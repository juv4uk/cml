[Reading 101 lines from start (total: 101 lines, 0 remaining)]

#![cfg(feature = "gpu-cuda")]

use cml::compute::{AdmissionBlocker, ComputeBackend, ComputeExecutionError, CpuComputeBackend};
use cml::gpu_cuda::CudaEmitError;
use cml::gpu_cuda_runtime::{CudaRuntimeError, CudaSession};
use cml::ir::{BufferLiteral, Ir, Params};

fn i32_map_ir(values: Vec<i32>, offset: i64) -> Ir {
    map_ir(BufferLiteral::I32(values), offset)
}

fn f32_map_ir(values: &[f32], offset: i64) -> Ir {
    map_ir(
        BufferLiteral::F32(values.iter().map(|value| value.to_bits()).collect()),
        offset,
    )
}

fn map_ir(buffer: BufferLiteral, offset: i64) -> Ir {
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["X".to_string()]),
                body: Box::new(Ir::App {
                    func: Box::new(Ir::Sid(sens::sens!(00001100))),
                    args: vec![Ir::Var("X".to_string()), Ir::Int(offset)],
                }),
            },
            Ir::Buffer(buffer),
        ],
    }
}

fn assert_cpu_cuda_parity(session: &CudaSession, ir: &Ir) {
    let cpu = CpuComputeBackend
        .execute(ir)
        .expect("CPU reference rejected admitted conformance case");
    let cuda = session
        .execute_map(ir)
        .expect("CUDA rejected admitted conformance case");

    assert_eq!(
        cuda.output, cpu,
        "CUDA output must exactly equal the CPU/reference output for the same admitted IR"
    );
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn live_cuda_matches_cpu_reference_matrix() {
    let session = CudaSession::new(0).expect("CUDA session creation failed");

    assert_cpu_cuda_parity(&session, &i32_map_ir(vec![1, 2, 3], 1));

    // 257 deliberately crosses a typical 256-thread launch boundary.
    // This witnesses the generated bounds guard on a non-block-aligned size.
    let non_aligned: Vec<i32> = (-128..129).collect();
    assert_eq!(non_aligned.len(), 257);
    assert_cpu_cuda_parity(&session, &i32_map_ir(non_aligned, 7));

    // Compare stored IEEE-754 binary32 bits, not decimal renderings or epsilon.
    // The CPU and CUDA mechanisms consume the same admitted Compute IR.
    assert_cpu_cuda_parity(
        &session,
        &f32_map_ir(&[1.0, -2.5, 0.1, -0.0, 16_777_216.0], 3),
    );
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn live_cuda_cannot_bypass_failed_admission() {
    let session = CudaSession::new(0).expect("CUDA session creation failed");
    assert_eq!(session.cached_kernel_count().unwrap(), 0);

    let ir = i32_map_ir(vec![i32::MAX], 1);

    let cpu = CpuComputeBackend
        .execute(&ir)
        .expect_err("CPU reference must reject unproven i32 overflow");
    assert!(matches!(
        cpu,
        ComputeExecutionError::NotEligible(blockers)
            if blockers.contains(&AdmissionBlocker::IntegerOverflowNotProven)
    ));

    let cuda = session
        .execute_map(&ir)
        .expect_err("CUDA must not bypass CML semantic admission");
    assert!(matches!(
        cuda,
        CudaRuntimeError::Emit(CudaEmitError::NotEligible(blockers))
            if blockers.contains(&AdmissionBlocker::IntegerOverflowNotProven)
    ));

    assert_eq!(
        session.cached_kernel_count().unwrap(),
        0,
        "rejected IR must not compile or cache a CUDA kernel"
    );
}

[executed on device: desktop (4fe47fce-cddf-44b3-9f53-0350f286048d)]