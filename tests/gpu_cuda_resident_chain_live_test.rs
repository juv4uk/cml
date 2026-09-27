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

fn resident_final_only(
    session: &CudaSession,
    functions: &[Ir],
    input: &BufferLiteral,
) -> (BufferLiteral, Duration) {
    let final_step = functions.len() - 1;
    let started = Instant::now();
    let execution = session
        .execute_map_chain_i32_selected(functions, input, &[final_step])
        .expect("selective resident CUDA chain failed");
    let elapsed = started.elapsed();
    assert_eq!(execution.outputs.len(), 1);
    let (step, output) = execution.outputs.into_iter().next().unwrap();
    assert_eq!(step, final_step);
    (output, elapsed)
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

    eprintln!("resident overflow error: {error:?}");
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

    eprintln!(
        "size,chain,roundtrip_ms,resident_all_ms,final_only_ms,roundtrip_over_final,all_over_final,dtoh_all_bytes,dtoh_final_bytes"
    );
    for &size in &[100_000usize, 1_000_000, 10_000_000] {
        let input = BufferLiteral::I32((0..size).map(|value| value as i32).collect());
        for &chain_len in &[2usize, 4, 8] {
            let functions = vec![add_one.clone(); chain_len];
            let (baseline_outputs, baseline_time) =
                baseline_roundtrip_chain(&session, &functions, &input);
            let (resident_outputs, resident_time) = resident_chain(&session, &functions, &input);
            let (final_output, final_time) = resident_final_only(&session, &functions, &input);
            assert_eq!(resident_outputs, baseline_outputs);
            assert_eq!(
                &final_output,
                baseline_outputs.last().expect("baseline final output")
            );

            let baseline_ms = baseline_time.as_secs_f64() * 1_000.0;
            let resident_ms = resident_time.as_secs_f64() * 1_000.0;
            let final_ms = final_time.as_secs_f64() * 1_000.0;
            let bytes_per_buffer = size * std::mem::size_of::<i32>();
            let dtoh_all_bytes = bytes_per_buffer * chain_len;
            let dtoh_final_bytes = bytes_per_buffer;
            eprintln!(
                "{size},{chain_len},{baseline_ms:.3},{resident_ms:.3},{final_ms:.3},{:.3},{:.3},{dtoh_all_bytes},{dtoh_final_bytes}",
                baseline_ms / final_ms,
                resident_ms / final_ms
            );
        }
    }
}
