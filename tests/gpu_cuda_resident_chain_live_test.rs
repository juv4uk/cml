#![cfg(feature = "gpu-cuda")]

use std::time::{Duration, Instant};

use cml::gpu_cuda_runtime::CudaSession;
use cml::ir::{BufferLiteral, Ir};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

fn map_ir(function: &Ir, input: BufferLiteral) -> Ir {
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![function.clone(), Ir::Buffer(input)],
    }
}

fn baseline_roundtrip_chain(
    session: &CudaSession,
    functions: &[Ir],
    input: &BufferLiteral,
) -> (Vec<BufferLiteral>, Duration) {
    let mut current = input.clone();
    let mut outputs = Vec::with_capacity(functions.len());
    let started = Instant::now();
    for function in functions {
        let execution = session
            .execute_map(&map_ir(function, current))
            .expect("baseline CUDA map failed");
        current = execution.output;
        outputs.push(current.clone());
    }
    (outputs, started.elapsed())
}

fn resident_chain(
    session: &CudaSession,
    functions: &[Ir],
    input: &BufferLiteral,
) -> (Vec<BufferLiteral>, Duration) {
    let started = Instant::now();
    let execution = session
        .execute_map_chain_i32(functions, input)
        .expect("resident CUDA chain failed");
    (execution.outputs, started.elapsed())
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn resident_chain_rejects_later_i32_overflow_before_execution() {
    use cml::gpu_cuda_runtime::CudaRuntimeError;

    let session = CudaSession::new(0).expect("CUDA session creation failed");
    let add_one = lower_one("(lambda (x) (+ x 1))");
    let functions = vec![add_one.clone(), add_one];
    let input = BufferLiteral::I32(vec![i32::MAX - 1]);

    let error = session
        .execute_map_chain_i32(&functions, &input)
        .expect_err("second map must be rejected before resident execution");

    assert_eq!(error.step, 1);
    assert!(matches!(error.source, CudaRuntimeError::UnsupportedInput));
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn owner_gtx_1050_ti_resident_chain_witness() {
    let session = CudaSession::new(0).expect("CUDA session creation failed");
    let add_one = lower_one("(lambda (x) (+ x 1))");

    // Compile the one kernel before timing either mechanism.
    let warm = map_ir(&add_one, BufferLiteral::I32(vec![1, 2, 3, 4]));
    session.execute_map(&warm).expect("CUDA warmup failed");

    eprintln!("size,chain,roundtrip_ms,resident_ms,roundtrip_over_resident");
    for &size in &[100_000usize, 1_000_000, 10_000_000] {
        let input = BufferLiteral::I32((0..size).map(|value| value as i32).collect());
        for &chain_len in &[2usize, 4, 8] {
            let functions = vec![add_one.clone(); chain_len];
            let (baseline_outputs, baseline_time) =
                baseline_roundtrip_chain(&session, &functions, &input);
            let (resident_outputs, resident_time) = resident_chain(&session, &functions, &input);
            assert_eq!(resident_outputs, baseline_outputs);

            let baseline_ms = baseline_time.as_secs_f64() * 1_000.0;
            let resident_ms = resident_time.as_secs_f64() * 1_000.0;
            eprintln!(
                "{size},{chain_len},{baseline_ms:.3},{resident_ms:.3},{:.3}",
                baseline_ms / resident_ms
            );
        }
    }
}
