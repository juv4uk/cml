//! Optional NVIDIA CUDA execution for admitted map regions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use cudarc::driver::{CudaContext, CudaFunction, CudaSlice, LaunchConfig, PushKernelArg};

use crate::accelerator::{
    AcceleratorApi, AcceleratorClass, AcceleratorDescriptor, AcceleratorVendor,
};
use crate::gpu_cuda::{emit_map_kernel, CudaEmitError};
use crate::ir::{BufferLiteral, Ir};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaDevice {
    pub descriptor: AcceleratorDescriptor,
    pub ordinal: usize,
    pub compute_capability: (i32, i32),
    pub total_memory_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaExecution {
    pub output: BufferLiteral,
    pub device: CudaDevice,
}

#[derive(Debug)]
pub enum CudaRuntimeError {
    Emit(CudaEmitError),
    UnsupportedInput,
    Nvrtc(String),
    Driver(String),
}

impl From<CudaEmitError> for CudaRuntimeError {
    fn from(error: CudaEmitError) -> Self {
        Self::Emit(error)
    }
}

fn nvrtc_arch_from_compute_capability((major, minor): (i32, i32)) -> String {
    format!("compute_{major}{minor}")
}

#[derive(Debug)]
struct ReusableBuffers<T> {
    len: usize,
    input: CudaSlice<T>,
    output: CudaSlice<T>,
}

/// Long-lived CUDA mechanism state for one device.
///
/// This owns no language semantics. It only keeps driver objects that are
/// expensive to reconstruct between equivalent admitted executions:
/// one CUDA context and compiled functions keyed by the emitted kernel source.
#[derive(Debug)]
pub struct CudaSession {
    context: Arc<CudaContext>,
    device: CudaDevice,
    kernels: Mutex<HashMap<String, CudaFunction>>,
    i32_buffers: Mutex<Option<ReusableBuffers<i32>>>,
    f32_buffers: Mutex<Option<ReusableBuffers<f32>>>,
}

impl CudaSession {
    pub fn new(device_ordinal: usize) -> Result<Self, CudaRuntimeError> {
        let context = CudaContext::new(device_ordinal)
            .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
        let device = device_evidence(&context)?;
        Ok(Self {
            context,
            device,
            kernels: Mutex::new(HashMap::new()),
            i32_buffers: Mutex::new(None),
            f32_buffers: Mutex::new(None),
        })
    }

    pub fn device(&self) -> &CudaDevice {
        &self.device
    }

    /// Number of distinct executable map kernels currently resident in this
    /// session. Exposed as mechanism evidence for reuse tests/benchmarks.
    pub fn cached_kernel_count(&self) -> Result<usize, CudaRuntimeError> {
        self.kernels
            .lock()
            .map(|kernels| kernels.len())
            .map_err(|error| {
                CudaRuntimeError::Driver(format!("kernel cache mutex poisoned: {error}"))
            })
    }

    pub fn execute_map(&self, ir: &Ir) -> Result<CudaExecution, CudaRuntimeError> {
        let source = emit_map_kernel(ir)?;
        let buffer = map_input(ir).ok_or(CudaRuntimeError::UnsupportedInput)?;
        if matches!(&buffer, BufferLiteral::I32(values) if values.is_empty())
            || matches!(&buffer, BufferLiteral::F32(values) if values.is_empty())
        {
            return Err(CudaRuntimeError::UnsupportedInput);
        }

        let function = self.function_for_source(source)?;
        let stream = self.context.default_stream();

        let output = match buffer {
            BufferLiteral::I32(input) => {
                let length =
                    u32::try_from(input.len()).map_err(|_| CudaRuntimeError::UnsupportedInput)?;
                let mut cache = self.i32_buffers.lock().map_err(|error| {
                    CudaRuntimeError::Driver(format!("i32 buffer cache mutex poisoned: {error}"))
                })?;
                if cache.as_ref().map(|buffers| buffers.len) != Some(input.len()) {
                    *cache = Some(ReusableBuffers {
                        len: input.len(),
                        input: unsafe { stream.alloc::<i32>(input.len()) }
                            .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?,
                        output: unsafe { stream.alloc::<i32>(input.len()) }
                            .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?,
                    });
                }
                let buffers = cache.as_mut().expect("buffer cache initialized");
                stream
                    .memcpy_htod(&input, &mut buffers.input)
                    .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
                unsafe {
                    stream
                        .launch_builder(&function)
                        .arg(&buffers.input)
                        .arg(&mut buffers.output)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
                let mut output = vec![0i32; input.len()];
                stream
                    .memcpy_dtoh(&buffers.output, &mut output)
                    .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
                BufferLiteral::I32(output)
            }
            BufferLiteral::F32(bits) => {
                let input: Vec<f32> = bits.into_iter().map(f32::from_bits).collect();
                let length =
                    u32::try_from(input.len()).map_err(|_| CudaRuntimeError::UnsupportedInput)?;
                let mut cache = self.f32_buffers.lock().map_err(|error| {
                    CudaRuntimeError::Driver(format!("f32 buffer cache mutex poisoned: {error}"))
                })?;
                if cache.as_ref().map(|buffers| buffers.len) != Some(input.len()) {
                    *cache = Some(ReusableBuffers {
                        len: input.len(),
                        input: unsafe { stream.alloc::<f32>(input.len()) }
                            .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?,
                        output: unsafe { stream.alloc::<f32>(input.len()) }
                            .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?,
                    });
                }
                let buffers = cache.as_mut().expect("buffer cache initialized");
                stream
                    .memcpy_htod(&input, &mut buffers.input)
                    .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
                unsafe {
                    stream
                        .launch_builder(&function)
                        .arg(&buffers.input)
                        .arg(&mut buffers.output)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
                let mut output = vec![0f32; input.len()];
                stream
                    .memcpy_dtoh(&buffers.output, &mut output)
                    .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
                BufferLiteral::F32(output.into_iter().map(f32::to_bits).collect())
            }
        };

        Ok(CudaExecution {
            output,
            device: self.device.clone(),
        })
    }

    fn function_for_source(&self, source: String) -> Result<CudaFunction, CudaRuntimeError> {
        let mut kernels = self.kernels.lock().map_err(|error| {
            CudaRuntimeError::Driver(format!("kernel cache mutex poisoned: {error}"))
        })?;
        if let Some(function) = kernels.get(&source) {
            return Ok(function.clone());
        }

        let arch = nvrtc_arch_from_compute_capability(self.device.compute_capability);
        let ptx = cudarc::nvrtc::compile_ptx_with_opts(
            source.clone(),
            cudarc::nvrtc::CompileOptions {
                options: vec![format!("-arch={arch}")],
                ..Default::default()
            },
        )
        .map_err(|error| CudaRuntimeError::Nvrtc(error.to_string()))?;
        let module = self
            .context
            .load_module(ptx)
            .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
        let function = module
            .load_function("cml_map")
            .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
        kernels.insert(source, function.clone());
        Ok(function)
    }
}

static CUDA_SESSIONS: OnceLock<Mutex<HashMap<usize, Arc<CudaSession>>>> = OnceLock::new();

pub fn discover_devices() -> Result<Vec<CudaDevice>, CudaRuntimeError> {
    let count =
        CudaContext::device_count().map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
    (0..count)
        .map(|ordinal| {
            let context = CudaContext::new(ordinal as usize)
                .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
            device_evidence(&context)
        })
        .collect()
}

/// Compatibility entrypoint. Repeated calls for the same ordinal now share a
/// long-lived CUDA session and its compiled-kernel cache.
pub fn execute_map(ir: &Ir, device_ordinal: usize) -> Result<CudaExecution, CudaRuntimeError> {
    session_for_device(device_ordinal)?.execute_map(ir)
}

fn session_for_device(device_ordinal: usize) -> Result<Arc<CudaSession>, CudaRuntimeError> {
    let sessions = CUDA_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut sessions = sessions.lock().map_err(|error| {
        CudaRuntimeError::Driver(format!("CUDA session cache mutex poisoned: {error}"))
    })?;
    if let Some(session) = sessions.get(&device_ordinal) {
        return Ok(session.clone());
    }

    let session = Arc::new(CudaSession::new(device_ordinal)?);
    sessions.insert(device_ordinal, session.clone());
    Ok(session)
}

fn device_evidence(context: &CudaContext) -> Result<CudaDevice, CudaRuntimeError> {
    let name = context
        .name()
        .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
    let compute_capability = context
        .compute_capability()
        .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
    let total_memory_bytes = context
        .total_mem()
        .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
    Ok(CudaDevice {
        descriptor: AcceleratorDescriptor {
            name,
            vendor: AcceleratorVendor::Nvidia,
            api: AcceleratorApi::Cuda,
            class: AcceleratorClass::DiscreteGpu,
        },
        ordinal: context.ordinal(),
        compute_capability,
        total_memory_bytes,
    })
}

fn map_input(ir: &Ir) -> Option<BufferLiteral> {
    let Ir::App { args, .. } = ir else {
        return None;
    };
    let [_, Ir::Buffer(buffer)] = args.as_slice() else {
        return None;
    };
    Some(buffer.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvrtc_arch_from_compute_capability_known_archs() {
        assert_eq!(nvrtc_arch_from_compute_capability((6, 1)), "compute_61");
        assert_eq!(nvrtc_arch_from_compute_capability((7, 0)), "compute_70");
        assert_eq!(nvrtc_arch_from_compute_capability((7, 5)), "compute_75");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 0)), "compute_80");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 6)), "compute_86");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 7)), "compute_87");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 9)), "compute_89");
        assert_eq!(nvrtc_arch_from_compute_capability((9, 0)), "compute_90");
    }

    #[test]
    fn nvrtc_arch_format_is_correct() {
        let arch = nvrtc_arch_from_compute_capability((6, 1));
        assert!(arch.starts_with("compute_"));
        assert_eq!(arch.len(), "compute_61".len());
        assert!(!arch.contains('.'));
    }
}
