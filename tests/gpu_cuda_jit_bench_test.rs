#![cfg(feature = "gpu-cuda")]

use cml::gpu_cuda_runtime::{
    CudaKernelMode, CudaSession, discover_devices, query_driver_version, query_nvrtc_version,
};
use cml::ir::{BufferLiteral, Ir, Params};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_expr(&expressions[0]).unwrap()
}

fn i32_map_ir(count: usize) -> Ir {
    let raw: Vec<i32> = (0..count).map(|x| (x % 1000) as i32).collect();
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["X".to_string()]),
                body: Box::new(Ir::App {
                    func: Box::new(Ir::Sid(sens::sens!(00001100))),
                    args: vec![Ir::Var("X".to_string()), Ir::Int(1)],
                }),
            },
            Ir::Buffer(BufferLiteral::I32(raw)),
        ],
    }
}

fn f32_map_ir(count: usize) -> Ir {
    let raw: Vec<f32> = (0..count).map(|x| (x % 1000) as f32).collect();
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["X".to_string()]),
                body: Box::new(Ir::App {
                    func: Box::new(Ir::Sid(sens::sens!(00001100))),
                    args: vec![Ir::Var("X".to_string()), Ir::Int(1)],
                }),
            },
            Ir::Buffer(BufferLiteral::F32(
                raw.iter().map(|f| f.to_bits()).collect(),
            )),
        ],
    }
}

#[test]
#[ignore = "requires live NVIDIA CUDA device"]
fn bench_cuda_jit_latency_breakdown_on_device_zero() {
    let devices = discover_devices().expect("CUDA discovery failed");
    assert!(!devices.is_empty(), "No CUDA devices discovered");
    let device = &devices[0];
    let driver_version = query_driver_version().unwrap_or(0);
    let nvrtc_version = query_nvrtc_version().unwrap_or((0, 0));

    eprintln!("============================================================");
    eprintln!("CUDA JIT LATENCY BREAKDOWN BENCHMARK (cml#491)");
    eprintln!(
        "Device: {} (ordinal {})",
        device.descriptor.name, device.ordinal
    );
    eprintln!(
        "Compute capability: {}.{}",
        device.compute_capability.0, device.compute_capability.1
    );
    eprintln!("Driver version: {}", driver_version);
    eprintln!("NVRTC version: {}.{}", nvrtc_version.0, nvrtc_version.1);
    eprintln!(
        "Total VRAM: {} MiB",
        device.total_memory_bytes / 1024 / 1024
    );
    eprintln!("============================================================");

    let session = CudaSession::new(0).expect("Session creation failed");

    // Negative control: unadmitted region fails closed before NVRTC
    let unadmitted_ir = lower_one("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(2147483647))");
    assert!(
        session
            .measure_latency_breakdown(&unadmitted_ir, CudaKernelMode::Production)
            .is_err(),
        "Negative control must fail closed"
    );
    eprintln!("[NEGATIVE CONTROL PASS] Unadmitted region rejected before NVRTC/JIT");

    let test_sizes = [1_000, 10_000, 100_000, 1_000_000];

    eprintln!("\n--- WORKLOAD: i32 MAP (Production Mode) ---");
    eprintln!(
        "size | lowering_us | nvrtc_us | driver_jit_us | fn_lookup_us | htod_us | kernel_us | dtoh_us | cold_ms | warm_us | cpu_ms | warm_speedup"
    );
    for size in test_sizes {
        let ir = i32_map_ir(size);
        let b = session
            .measure_latency_breakdown(&ir, CudaKernelMode::Production)
            .expect("Breakdown measurement failed");
        eprintln!(
            "{:<6} | {:>11.1} | {:>8.1} | {:>13.1} | {:>12.1} | {:>7.1} | {:>9.1} | {:>7.1} | {:>7.2} | {:>7.1} | {:>6.2} | {:>12.2}x",
            size,
            b.cml_ir_lowering_ns as f64 / 1_000.0,
            b.nvrtc_compile_ns as f64 / 1_000.0,
            b.driver_jit_load_ns as f64 / 1_000.0,
            b.function_lookup_ns as f64 / 1_000.0,
            b.htod_transfer_ns as f64 / 1_000.0,
            b.kernel_execution_ns as f64 / 1_000.0,
            b.dtoh_transfer_ns as f64 / 1_000.0,
            b.total_cold_ns as f64 / 1_000_000.0,
            b.total_warm_ns as f64 / 1_000.0,
            b.cpu_reference_ns as f64 / 1_000_000.0,
            b.warm_speedup_over_cpu(),
        );
    }

    eprintln!("\n--- WORKLOAD: f32 MAP (BitwiseEquality Mode -fmad=false) ---");
    eprintln!(
        "size | lowering_us | nvrtc_us | driver_jit_us | fn_lookup_us | htod_us | kernel_us | dtoh_us | cold_ms | warm_us | cpu_ms | warm_speedup"
    );
    for size in test_sizes {
        let ir = f32_map_ir(size);
        let b = session
            .measure_latency_breakdown(&ir, CudaKernelMode::BitwiseEquality)
            .expect("Breakdown measurement failed");
        eprintln!(
            "{:<6} | {:>11.1} | {:>8.1} | {:>13.1} | {:>12.1} | {:>7.1} | {:>9.1} | {:>7.1} | {:>7.2} | {:>7.1} | {:>6.2} | {:>12.2}x",
            size,
            b.cml_ir_lowering_ns as f64 / 1_000.0,
            b.nvrtc_compile_ns as f64 / 1_000.0,
            b.driver_jit_load_ns as f64 / 1_000.0,
            b.function_lookup_ns as f64 / 1_000.0,
            b.htod_transfer_ns as f64 / 1_000.0,
            b.kernel_execution_ns as f64 / 1_000.0,
            b.dtoh_transfer_ns as f64 / 1_000.0,
            b.total_cold_ns as f64 / 1_000_000.0,
            b.total_warm_ns as f64 / 1_000.0,
            b.cpu_reference_ns as f64 / 1_000_000.0,
            b.warm_speedup_over_cpu(),
        );
    }
}
