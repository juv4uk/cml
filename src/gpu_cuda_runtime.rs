//! Optional NVIDIA CUDA execution for admitted map regions.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use cudarc::driver::{CudaContext, CudaFunction, CudaSlice, LaunchConfig, PushKernelArg};

use crate::accelerator::{
    AcceleratorApi, AcceleratorClass, AcceleratorDescriptor, AcceleratorVendor,
};
use crate::compute::{I32Range, i32_buffer_range, prove_i32_map_range};
use crate::gpu_cuda::{CudaEmitError, emit_map_kernel};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaChainExecution {
    pub outputs: Vec<BufferLiteral>,
    pub device: CudaDevice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaSelectedChainExecution {
    /// Host-materialized outputs paired with their zero-based chain step.
    pub outputs: Vec<(usize, BufferLiteral)>,
    pub device: CudaDevice,
}

#[derive(Debug)]
pub enum CudaRuntimeError {
    Emit(CudaEmitError),
    UnsupportedInput,
    InsufficientDeviceMemory {
        required_bytes: usize,
        free_bytes: usize,
    },
    InvalidMaterializationStep {
        step: usize,
        chain_len: usize,
    },
    Nvrtc(String),
    Driver(String),
}

#[derive(Debug)]
pub struct CudaChainRuntimeError {
    pub step: usize,
    pub source: CudaRuntimeError,
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
        if matches!(buffer, BufferLiteral::I32(values) if values.is_empty())
            || matches!(buffer, BufferLiteral::F32(values) if values.is_empty())
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
                    .memcpy_htod(input.as_slice(), &mut buffers.input)
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
                let input: Vec<f32> = bits.iter().copied().map(f32::from_bits).collect();
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

    /// Execute a linear i32 map chain while intermediate buffers remain on the
    /// selected CUDA device. Every step is re-admitted from a carried range
    /// proof; no kernel is allowed to bypass the checked-add overflow guard.
    ///
    /// Compatibility path: every chain step is materialized on the host.
    pub fn execute_map_chain_i32(
        &self,
        functions: &[Ir],
        input: &BufferLiteral,
    ) -> Result<CudaChainExecution, CudaChainRuntimeError> {
        let requested_steps: Vec<usize> = (0..functions.len()).collect();
        let selected = self.execute_map_chain_i32_selected(functions, input, &requested_steps)?;
        Ok(CudaChainExecution {
            outputs: selected
                .outputs
                .into_iter()
                .map(|(_, output)| output)
                .collect(),
            device: selected.device,
        })
    }

    /// Execute a linear i32 map chain and materialize only selected chain steps.
    ///
    /// Device storage is bounded to the original input plus two ping-pong
    /// scratch buffers, independent of chain length. requested_steps is a host
    /// execution policy only; it does not alter SENS semantics or admission.
    pub fn execute_map_chain_i32_selected(
        &self,
        functions: &[Ir],
        input: &BufferLiteral,
        requested_steps: &[usize],
    ) -> Result<CudaSelectedChainExecution, CudaChainRuntimeError> {
        let fail = |step, source| CudaChainRuntimeError { step, source };
        let BufferLiteral::I32(input_values) = input else {
            return Err(fail(0, CudaRuntimeError::UnsupportedInput));
        };
        if functions.is_empty() || input_values.is_empty() {
            return Err(fail(0, CudaRuntimeError::UnsupportedInput));
        }

        let requested: HashSet<usize> = requested_steps.iter().copied().collect();
        if let Some(&step) = requested.iter().find(|&&step| step >= functions.len()) {
            return Err(fail(
                step,
                CudaRuntimeError::InvalidMaterializationStep {
                    step,
                    chain_len: functions.len(),
                },
            ));
        }

        let length = u32::try_from(input_values.len())
            .map_err(|_| fail(0, CudaRuntimeError::UnsupportedInput))?;
        let element_bytes = input_values
            .len()
            .checked_mul(std::mem::size_of::<i32>())
            .ok_or_else(|| fail(0, CudaRuntimeError::UnsupportedInput))?;
        // One immutable uploaded input plus two ping-pong scratch buffers.
        let required_bytes = element_bytes
            .checked_mul(3)
            .ok_or_else(|| fail(0, CudaRuntimeError::UnsupportedInput))?;

        self.context
            .bind_to_thread()
            .map_err(|error| fail(0, CudaRuntimeError::Driver(error.to_string())))?;
        let (free_bytes, _) = cudarc::driver::result::mem_get_info()
            .map_err(|error| fail(0, CudaRuntimeError::Driver(error.to_string())))?;
        if required_bytes > free_bytes {
            return Err(fail(
                0,
                CudaRuntimeError::InsufficientDeviceMemory {
                    required_bytes,
                    free_bytes,
                },
            ));
        }

        // Re-admit the complete chain before the first kernel launch.
        let mut range =
            i32_buffer_range(input).ok_or_else(|| fail(0, CudaRuntimeError::UnsupportedInput))?;
        let mut prepared = Vec::with_capacity(functions.len());
        for (step, function_ir) in functions.iter().enumerate() {
            let next_range = prove_i32_map_range(function_ir, range)
                .ok_or_else(|| fail(step, CudaRuntimeError::UnsupportedInput))?;
            let probe = i32_range_probe_ir(function_ir, range);
            let source = emit_map_kernel(&probe)
                .map_err(|error| fail(step, CudaRuntimeError::Emit(error)))?;
            let function = self
                .function_for_source(source)
                .map_err(|error| fail(step, error))?;
            prepared.push(function);
            range = next_range;
        }

        let stream = self.context.default_stream();
        let input_device = stream
            .clone_htod(input_values.as_slice())
            .map_err(|error| fail(0, CudaRuntimeError::Driver(error.to_string())))?;
        let mut ping = unsafe { stream.alloc::<i32>(input_values.len()) }
            .map_err(|error| fail(0, CudaRuntimeError::Driver(error.to_string())))?;
        let mut pong = unsafe { stream.alloc::<i32>(input_values.len()) }
            .map_err(|error| fail(0, CudaRuntimeError::Driver(error.to_string())))?;

        let mut outputs = Vec::with_capacity(requested.len());
        for (step, function) in prepared.iter().enumerate() {
            if step == 0 {
                unsafe {
                    stream
                        .launch_builder(function)
                        .arg(&input_device)
                        .arg(&mut ping)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| fail(step, CudaRuntimeError::Driver(error.to_string())))?;
                if requested.contains(&step) {
                    let mut output = vec![0i32; input_values.len()];
                    stream
                        .memcpy_dtoh(&ping, &mut output)
                        .map_err(|error| fail(step, CudaRuntimeError::Driver(error.to_string())))?;
                    outputs.push((step, BufferLiteral::I32(output)));
                }
            } else if step % 2 == 1 {
                unsafe {
                    stream
                        .launch_builder(function)
                        .arg(&ping)
                        .arg(&mut pong)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| fail(step, CudaRuntimeError::Driver(error.to_string())))?;
                if requested.contains(&step) {
                    let mut output = vec![0i32; input_values.len()];
                    stream
                        .memcpy_dtoh(&pong, &mut output)
                        .map_err(|error| fail(step, CudaRuntimeError::Driver(error.to_string())))?;
                    outputs.push((step, BufferLiteral::I32(output)));
                }
            } else {
                unsafe {
                    stream
                        .launch_builder(function)
                        .arg(&pong)
                        .arg(&mut ping)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| fail(step, CudaRuntimeError::Driver(error.to_string())))?;
                if requested.contains(&step) {
                    let mut output = vec![0i32; input_values.len()];
                    stream
                        .memcpy_dtoh(&ping, &mut output)
                        .map_err(|error| fail(step, CudaRuntimeError::Driver(error.to_string())))?;
                    outputs.push((step, BufferLiteral::I32(output)));
                }
            }
        }

        Ok(CudaSelectedChainExecution {
            outputs,
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

pub fn execute_map_chain_i32(
    functions: &[Ir],
    input: &BufferLiteral,
    device_ordinal: usize,
) -> Result<CudaChainExecution, CudaChainRuntimeError> {
    let session = session_for_device(device_ordinal)
        .map_err(|source| CudaChainRuntimeError { step: 0, source })?;
    session.execute_map_chain_i32(functions, input)
}

pub fn execute_map_chain_i32_selected(
    functions: &[Ir],
    input: &BufferLiteral,
    requested_steps: &[usize],
    device_ordinal: usize,
) -> Result<CudaSelectedChainExecution, CudaChainRuntimeError> {
    let session = session_for_device(device_ordinal)
        .map_err(|source| CudaChainRuntimeError { step: 0, source })?;
    session.execute_map_chain_i32_selected(functions, input, requested_steps)
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

fn map_input(ir: &Ir) -> Option<&BufferLiteral> {
    let Ir::App { args, .. } = ir else {
        return None;
    };
    let [_, Ir::Buffer(buffer)] = args.as_slice() else {
        return None;
    };
    Some(buffer)
}

fn i32_range_probe_ir(function: &Ir, range: I32Range) -> Ir {
    let values = if range.min == range.max {
        vec![range.min]
    } else {
        vec![range.min, range.max]
    };
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![function.clone(), Ir::Buffer(BufferLiteral::I32(values))],
    }
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
