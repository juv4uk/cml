//! Optional NVIDIA CUDA execution for admitted map regions.

use cudarc::driver::{CudaContext, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::compile_ptx;

use crate::gpu_cuda::{CudaEmitError, emit_map_kernel};
use crate::ir::{BufferLiteral, Ir};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaExecution {
    pub output: BufferLiteral,
    pub device_ordinal: usize,
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

pub fn execute_map(ir: &Ir, device_ordinal: usize) -> Result<CudaExecution, CudaRuntimeError> {
    let source = emit_map_kernel(ir)?;
    let buffer = map_input(ir).ok_or(CudaRuntimeError::UnsupportedInput)?;
    if matches!(&buffer, BufferLiteral::I32(values) if values.is_empty())
        || matches!(&buffer, BufferLiteral::F32(values) if values.is_empty())
    {
        return Err(CudaRuntimeError::UnsupportedInput);
    }

    let ptx = compile_ptx(source).map_err(|error| CudaRuntimeError::Nvrtc(error.to_string()))?;
    let context = CudaContext::new(device_ordinal)
        .map_err(|error| CudaRuntimeError::Driver(error.to_string()))?;
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

    Ok(CudaExecution {
        output,
        device_ordinal,
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
