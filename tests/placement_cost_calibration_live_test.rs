#![cfg(feature = "gpu-cuda")]

use std::hint::black_box;
use std::time::{Duration, Instant};

use cml::compute::{ComputeBackend, ParallelCpuComputeBackend};
use cml::gpu_cuda_runtime::CudaSession;
use cml::ir::{BufferLiteral, Ir, Params};

fn checked_add_body(operation_count: usize) -> Ir {
    (0..operation_count).fold(Ir::Var("X".to_string()), |body, _| Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(00001100))),
        args: vec![body, Ir::Int(1)],
    })
}

fn map_ir(count: usize, operation_count: usize) -> Ir {
    let raw: Vec<i32> = (0..count).map(|x| (x % 1000) as i32).collect();
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["X".to_string()]),
                body: Box::new(checked_add_body(operation_count)),
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
    (
        median(times),
        last.expect("at least one calibration repetition"),
    )
}

#[test]
#[ignore = "owner-hardware calibration: requires live NVIDIA CUDA device"]
fn owner_i5_6400_gtx_1050_ti_placement_cost_matrix() {
    let sizes = [1_000usize, 10_000, 100_000, 1_000_000, 10_000_000];
    let operation_counts = [1usize, 2, 4, 8];
    let cpu4 = ParallelCpuComputeBackend::new(4);
    let cuda = CudaSession::new(0).expect("CUDA warm session creation failed");

    // Each operation count emits a distinct scalar expression. Pre-warm those
    // kernels once so this calibration measures steady-state mechanism cost,
    // not context creation or first compile/load.
    for operation_count in operation_counts {
        let warmup = map_ir(1, operation_count);
        cuda.execute_map(&warmup)
            .expect("CUDA placement-cost prewarm failed");
    }

    println!("size,ops,cpu4_ms,cuda_warm_ms,cpu4_over_cuda");

    for operation_count in operation_counts {
        for count in sizes {
            let ir = map_ir(count, operation_count);

            let (cpu4_time, cpu4_output) = measure(3, || cpu4.execute(&ir).unwrap());
            let (cuda_time, cuda_output) = measure(3, || cuda.execute_map(&ir).unwrap());

            assert_eq!(
                cpu4_output, cuda_output.output,
                "CPU/GPU parity failed for size={count} ops={operation_count}"
            );

            let speedup = cpu4_time.as_secs_f64() / cuda_time.as_secs_f64();
            println!(
                "{count},{operation_count},{:.3},{:.3},{speedup:.3}",
                cpu4_time.as_secs_f64() * 1_000.0,
                cuda_time.as_secs_f64() * 1_000.0,
            );
        }
    }
}
