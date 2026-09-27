//! Optional NVIDIA CUDA execution for admitted map regions.

use cudarc::driver::{CudaContext, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::compile_ptx;

use crate::accelerator::{
    AcceleratorApi, AcceleratorClass, AcceleratorDescriptor, AcceleratorVendor,
};
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

// Derive NVRTC arch string from compute capability (major, minor).
// Format: "compute_<major><minor>" e.g., compute_61, compute_75, compute_86.
fn nvrtc_arch_from_compute_capability(cc: (i32, i32)) -> String {
    format!("compute_{}{}", cc.0, cc.1)
}

pub fn execute_map(ir: &Ir, device_ordinal: usize) -> Result<CudaExecution, CudaRuntimeError> {
    let source = emit_map_kernel(ir)?;
    let buffer = map_input(ir).ok_or(CudaRuntimeError::UnsupportedInput)?;
    if matches!(&buffer, BufferLiteral::I32(values) if values.is_empty())
        || matches!(&buffer, BufferLiteral::F32(values) if values.is_empty())
    {
        return Err(CudaRuntimeError::UnsupportedInput);
    }

    // Create context first to get the actual device's compute capability.
    let context = CudaContext::new(device_ordinal)
        .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
    let device = device_evidence(&context)?;

    // Derive NVRTC arch from the actual device's compute capability.
    let arch = nvrtc_arch_from_compute_capability(device.compute_capability);
    let ptx = cudarc::nvrtc::compile_ptx_with_opts(
        source,
        cudarc::nvrtc::CompileOptions {
            options: vec![format!("-arch={}", arch)],
            ..Default::default()
        },
    )
    .map_err(|error| CudaRuntimeError::Nvrtc(error.to_string()))?;
    let stream = context.default_stream();
    let module = context
        .load_module(ptx)
        .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
    let function = module
        .load_function("cml_map")
        .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;

    let output = match buffer {
        BufferLiteral::I32(input) => {
            let input_device = stream
                .clone_htod(&input)
                .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
            let mut output_device = stream
                .alloc_zeros::<i32>(input.len())
                .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
            let length =
                u32::try_from(input.len()).map_err(|_| CudaRuntimeError::UnsupportedInput)?;
            unsafe {
                stream
                    .launch_builder(&function)
                    .arg(&input_device)
                    .arg(&mut output_device)
                    .arg(&length)
                    .launch(LaunchConfig::for_num_elems(length))
            }
            .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
            BufferLiteral::I32(
                stream
                    .clone_dtoh(&output_device)
                    .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?,
            )
        }
        BufferLiteral::F32(bits) => {
            let input: Vec<f32> = bits.into_iter().map(f32::from_bits).collect();
            let input_device = stream
                .clone_htod(&input)
                .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
            let mut output_device = stream
                .alloc_zeros::<f32>(input.len())
                .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
            let length =
                u32::try_from(input.len()).map_err(|_| CudaRuntimeError::UnsupportedInput)?;
            unsafe {
                stream
                    .launch_builder(&function)
                    .arg(&input_device)
                    .arg(&mut output_device)
                    .arg(&length)
                    .launch(LaunchConfig::for_num_elems(length))
            }
            .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
            let output: Vec<f32> = stream
                .clone_dtoh(&output_device)
                .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
            BufferLiteral::F32(output.into_iter().map(f32::to_bits).collect())
        }
    };

    Ok(CudaExecution { output, device })
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
        // GTX 1050 Ti / Pascal
        assert_eq!(nvrtc_arch_from_compute_capability((6, 1)), "compute_61");
        // V100 / Volta
        assert_eq!(nvrtc_arch_from_compute_capability((7, 0)), "compute_70");
        assert_eq!(nvrtc_arch_from_compute_capability((7, 5)), "compute_75");
        // A100 / Ampere
        assert_eq!(nvrtc_arch_from_compute_capability((8, 0)), "compute_80");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 6)), "compute_86");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 7)), "compute_87");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 9)), "compute_89");
        // H100 / Hopper
        assert_eq!(nvrtc_arch_from_compute_capability((9, 0)), "compute_90");
    }

    #[test]
    fn nvrtc_arch_format_is_correct() {
        // Format must be exactly "compute_<major><minor>" for NVRTC
        let arch = nvrtc_arch_from_compute_capability((6, 1));
        assert!(arch.starts_with("compute_"));
        assert_eq!(arch.len(), "compute_61".len());
        assert!(!arch.contains('.'));
    }
}
