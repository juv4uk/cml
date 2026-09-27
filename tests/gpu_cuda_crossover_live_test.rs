#![cfg(feature = "gpu-cuda")]

use std::hint::black_box;
use std::time::{Duration, Instant};

use cml::compute::{ComputeBackend, CpuComputeBackend, ParallelCpuComputeBackend};
use cml::gpu_cuda_runtime::CudaSession;
use cml::ir::{BufferLiteral, Ir, Params};

fn map_ir(count: usize) -> Ir {
    let raw: Vec<i32> = (0..count).map(|x| (x % 1000) as i32).collect();
    Ir::App {
        func: Box::new(Ir::Sid(sens::sid!(01011001))),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["X".to_string()]),
                body: Box::new(Ir::App {
                    func: Box::new(Ir::Sid(sens::sid!(00001100))),
                    args: vec![Ir::Var("X".to_string()), Ir::Int(1)],
                }),
            },
            Ir::Buffer(BufferLiteral::I32(raw)),
        ],
    }
}

fn median(mut values: Vec<Duration>) -> Duration {
    values.sort_unstable();
    values[values.len() / 2]
}

fn measure<F, T>(reps: usize, mut f: F) -> (Duration, T)
where
    F: FnMut() -> T,
{
    let mut times = Vec::with_capacity(reps);
    let mut last = None;
    for _ in 0..reps {
        let start = Instant::now();
        let value = black_box(f());
        times.push(start.elapsed());
        last = Some(value);
    }
    (median(times), last.unwrap())
}

#[test]
#[ignore = "owner-hardware calibration: requires live NVIDIA CUDA device"]
fn owner_i5_6400_gtx_1050_ti_i32_map_crossover() {
    let sizes = [1_000usize, 10_000, 100_000, 1_000_000, 10_000_000];
    let cpu1 = CpuComputeBackend;
    let cpu4 = ParallelCpuComputeBackend::new(4);
    let warm = CudaSession::new(0).expect("CUDA warm session creation failed");

    // Compile/load the x+1 kernel once before warm measurements.
    let prewarm = map_ir(1);
    warm.execute_map(&prewarm).expect("CUDA prewarm failed");
    assert_eq!(warm.cached_kernel_count().unwrap(), 1);

    println!("size,cpu1_ms,cpu4_ms,cuda_cold_ms,cuda_warm_ms,cpu4_over_cuda_warm");

    for count in sizes {
        let ir = map_ir(count);

        let (cpu1_time, cpu1_output) = measure(3, || cpu1.execute(&ir).unwrap());
        let (cpu4_time, cpu4_output) = measure(3, || cpu4.execute(&ir).unwrap());
        assert_eq!(cpu1_output, cpu4_output);

        let cold_start = Instant::now();
        let cold_session = CudaSession::new(0).expect("cold CUDA session creation failed");
        let cold_output = cold_session.execute_map(&ir).expect("cold CUDA map failed");
        let cold_time = cold_start.elapsed();
        assert_eq!(cpu1_output, cold_output.output);

        let (warm_time, warm_output) = measure(5, || warm.execute_map(&ir).unwrap());
        assert_eq!(cpu1_output, warm_output.output);
        assert_eq!(warm.cached_kernel_count().unwrap(), 1);

        let speedup = cpu4_time.as_secs_f64() / warm_time.as_secs_f64();
        println!(
            "{count},{:.3},{:.3},{:.3},{:.3},{speedup:.3}",
            cpu1_time.as_secs_f64() * 1_000.0,
            cpu4_time.as_secs_f64() * 1_000.0,
            cold_time.as_secs_f64() * 1_000.0,
            warm_time.as_secs_f64() * 1_000.0,
        );
    }
}
