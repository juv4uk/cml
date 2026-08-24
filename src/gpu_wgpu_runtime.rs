//! Optional native `wgpu` execution for semantically admitted map regions.
//!
//! This layer owns physical adapter selection, storage-buffer transfer,
//! dispatch, and readback. It cannot admit an IR region: shader emission still
//! passes through `gpu_wgsl`, whose fail-closed analysis is authoritative.

use std::sync::mpsc;
use std::time::Duration;

use crate::gpu_wgsl::{WgslError, emit_map_shader};
use crate::ir::{BufferLiteral, Ir};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterEvidence {
    pub name: String,
    pub vendor: u32,
    pub device: u32,
    pub device_type: wgpu::DeviceType,
    pub backend: wgpu::Backend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuExecution {
    pub output: BufferLiteral,
    pub adapter: AdapterEvidence,
}

#[derive(Debug)]
pub enum WgpuRuntimeError {
    Shader(WgslError),
    UnsupportedInput,
    NoAdapter(String),
    RequestDevice(String),
    Poll(String),
    Map(String),
    ReadbackClosed,
}

impl From<WgslError> for WgpuRuntimeError {
    fn from(error: WgslError) -> Self {
        Self::Shader(error)
    }
}

/// Execute one already-admissible numeric-buffer map on a `wgpu` adapter.
///
/// Returning `AdapterEvidence` is intentional: a successful dispatch proves a
/// wgpu execution path, while the adapter metadata is the separate evidence
/// needed before calling that path physical GPU execution.
pub async fn execute_map(ir: &Ir) -> Result<WgpuExecution, WgpuRuntimeError> {
    let shader_source = emit_map_shader(ir)?;
    let input = map_input(ir).ok_or(WgpuRuntimeError::UnsupportedInput)?;
    if input.bytes.is_empty() {
        return Err(WgpuRuntimeError::UnsupportedInput);
    }

    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        })
        .await
        .map_err(|error| WgpuRuntimeError::NoAdapter(error.to_string()))?;
    let info = adapter.get_info();
    let evidence = AdapterEvidence {
        name: info.name,
        vendor: info.vendor,
        device: info.device,
        device_type: info.device_type,
        backend: info.backend,
    };

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("cml-compute-device"),
            ..Default::default()
        })
        .await
        .map_err(|error| WgpuRuntimeError::RequestDevice(error.to_string()))?;

    let byte_len = input.bytes.len() as wgpu::BufferAddress;
    let input_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cml-compute-input"),
        size: byte_len,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cml-compute-output"),
        size: byte_len,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cml-compute-readback"),
        size: byte_len,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&input_buffer, 0, &input.bytes);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cml-compute-map-wgsl"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("cml-compute-map-pipeline"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cml-compute-map-bind-group"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output_buffer.as_entire_binding(),
            },
        ],
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("cml-compute-map-commands"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("cml-compute-map-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(input.element_count.div_ceil(64), 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output_buffer, 0, &readback, 0, byte_len);
    queue.submit([encoder.finish()]);

    let slice = readback.slice(..);
    let (sender, receiver) = mpsc::sync_channel(1);
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .map_err(|error| WgpuRuntimeError::Poll(error.to_string()))?;
    receiver
        .recv()
        .map_err(|_| WgpuRuntimeError::ReadbackClosed)?
        .map_err(|error| WgpuRuntimeError::Map(error.to_string()))?;

    let mapped = slice
        .get_mapped_range()
        .map_err(|error| WgpuRuntimeError::Map(error.to_string()))?;
    let output = input.decode(&mapped)?;
    drop(mapped);
    readback.unmap();
    Ok(WgpuExecution {
        output,
        adapter: evidence,
    })
}

/// Native convenience entry point for CLI tools and evidence probes.
pub fn execute_map_blocking(ir: &Ir) -> Result<WgpuExecution, WgpuRuntimeError> {
    pollster::block_on(execute_map(ir))
}

struct InputBytes {
    bytes: Vec<u8>,
    element_count: u32,
    kind: InputKind,
}

#[derive(Clone, Copy)]
enum InputKind {
    I32,
    F32,
}

impl InputBytes {
    fn decode(&self, bytes: &[u8]) -> Result<BufferLiteral, WgpuRuntimeError> {
        if bytes.len() != self.bytes.len() || !bytes.len().is_multiple_of(4) {
            return Err(WgpuRuntimeError::Map("unexpected readback length".into()));
        }
        let words = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().expect("four-byte chunk")));
        Ok(match self.kind {
            InputKind::I32 => BufferLiteral::I32(words.map(|word| word as i32).collect()),
            InputKind::F32 => BufferLiteral::F32(words.collect()),
        })
    }
}

fn map_input(ir: &Ir) -> Option<InputBytes> {
    let Ir::App { args, .. } = ir else {
        return None;
    };
    let [_, Ir::Buffer(buffer)] = args.as_slice() else {
        return None;
    };
    let (bytes, element_count, kind) = match buffer {
        BufferLiteral::I32(values) => (
            values
                .iter()
                .flat_map(|value| value.to_ne_bytes())
                .collect(),
            values.len(),
            InputKind::I32,
        ),
        BufferLiteral::F32(bits) => (
            bits.iter().flat_map(|value| value.to_ne_bytes()).collect(),
            bits.len(),
            InputKind::F32,
        ),
    };
    Some(InputBytes {
        bytes,
        element_count: element_count.try_into().ok()?,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_ir(buffer: BufferLiteral) -> Ir {
        Ir::App {
            func: Box::new(Ir::Var("NUMERIC-BUFFER-MAP".into())),
            args: vec![Ir::Nil, Ir::Buffer(buffer)],
        }
    }

    #[test]
    fn i32_storage_abi_round_trips_native_words() {
        let input = map_input(&map_ir(BufferLiteral::I32(vec![-1, 0, 42]))).unwrap();
        assert_eq!(input.element_count, 3);
        assert_eq!(
            input.decode(&input.bytes).unwrap(),
            BufferLiteral::I32(vec![-1, 0, 42])
        );
    }

    #[test]
    fn f32_storage_abi_preserves_bits() {
        let bits = vec![0x8000_0000, 0x3f80_0000, 0x7fc0_0042];
        let input = map_input(&map_ir(BufferLiteral::F32(bits.clone()))).unwrap();
        assert_eq!(
            input.decode(&input.bytes).unwrap(),
            BufferLiteral::F32(bits)
        );
    }

    #[test]
    fn malformed_readback_is_rejected() {
        let input = map_input(&map_ir(BufferLiteral::I32(vec![1]))).unwrap();
        assert!(matches!(
            input.decode(&[0, 1]),
            Err(WgpuRuntimeError::Map(_))
        ));
    }
}
