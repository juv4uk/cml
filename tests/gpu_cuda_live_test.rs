[Reading 185 lines from start (total: 185 lines, 0 remaining)]

#![cfg(feature = "gpu-cuda")]

use cml::accelerator::{AcceleratorApi, AcceleratorVendor, SelectionPolicy, select_accelerator};
use cml::compute::{AdmissionBlocker, ComputeBackend, ComputeExecutionError, CpuComputeBackend};
use cml::gpu_cuda::CudaEmitError;
use cml::gpu_cuda_runtime::{
    CudaCapabilityStatus, CudaRuntimeError, CudaSession, discover_devices, execute_map,
    probe_capability,
};
use cml::ir::{BufferLiteral, Ir, Params};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_expr(&expressions[0]).unwrap()
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

fn i32_map_ir(values: Vec<i32>, offset: i64) -> Ir {
    map_ir(BufferLiteral::I32(values), offset)
}

fn f32_map_ir(values: &[f32], offset: i64) -> Ir {
    map_ir(
        BufferLiteral::F32(values.iter().map(|value| value.to_bits()).collect()),
        offset,
    )
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
fn capability_probe_reports_live_cuda_devices() {
    let status = probe_capability().expect("CUDA capability probe failed");
    let CudaCapabilityStatus::Live(devices) = status else {
        panic!("CUDA runtime was present but reported zero devices");
    };
    assert!(!devices.is_empty());
    eprintln!("CUDA capability evidence: {devices:?}");
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn admitted_i32_map_executes_on_cuda_device_zero() {
    let devices = discover_devices().expect("CUDA discovery failed");
    assert_eq!(devices.len(), 1);
    let descriptors = [devices[0].descriptor.clone()];
    let selected = select_accelerator(&descriptors, SelectionPolicy::VendorOptimized)
        .expect("planner rejected live CUDA descriptor");
    assert_eq!(selected.vendor, AcceleratorVendor::Nvidia);
    assert_eq!(selected.api, AcceleratorApi::Cuda);

    let ir = lower_one("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))");
    let execution = execute_map(&ir, 0).expect("live CUDA execution failed");
    eprintln!("CUDA device evidence: {:?}", execution.device);
    assert_eq!(execution.device.ordinal, 0);
    assert_eq!(execution.device.descriptor, *selected);
    assert_eq!(execution.output, BufferLiteral::I32(vec![2, 3, 4]));
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn one_session_reuses_one_kernel_across_different_buffers() {
    let session = CudaSession::new(0).expect("CUDA session creation failed");
    assert_eq!(session.cached_kernel_count().unwrap(), 0);

    let first = lower_one("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))");
    let first = session.execute_map(&first).expect("first CUDA map failed");
    assert_eq!(first.output, BufferLiteral::I32(vec![2, 3, 4]));
    assert_eq!(session.cached_kernel_count().unwrap(), 1);

    let second =
        lower_one("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(10 20 30 40 50 60 70 80))");
    let second = session
        .execute_map(&second)
        .expect("second CUDA map failed");
    assert_eq!(
        second.output,
        BufferLiteral::I32(vec![11, 21, 31, 41, 51, 61, 71, 81])
    );
    assert_eq!(
        session.cached_kernel_count().unwrap(),
        1,
        "buffer values/length must not create a second compiled kernel"
    );
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn prepared_map_reuses_one_admission_witness_for_immutable_ir() {
    let session = CudaSession::new(0).expect("CUDA session creation failed");
    let ir = lower_one("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(5 6 7 8))");
    let prepared = session.prepare_map(&ir).expect("CUDA preparation failed");

    let first = prepared.execute().expect("first prepared execution failed");
    let second = prepared
        .execute()
        .expect("second prepared execution failed");

    assert_eq!(first.output, BufferLiteral::I32(vec![6, 7, 8, 9]));
    assert_eq!(second.output, first.output);
    assert_eq!(session.cached_kernel_count().unwrap(), 1);
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn live_cuda_matches_cpu_reference_matrix() {
    let session = CudaSession::new(0).expect("CUDA session creation failed");

    assert_cpu_cuda_parity(&session, &i32_map_ir(vec![1, 2, 3], 1));

    // 257 crosses a typical 256-thread launch boundary and witnesses the
    // generated bounds guard on a non-block-aligned size.
    let non_aligned: Vec<i32> = (-128..129).collect();
    assert_eq!(non_aligned.len(), 257);
    assert_cpu_cuda_parity(&session, &i32_map_ir(non_aligned, 7));

    // Compare stored IEEE-754 binary32 bits, not decimal renderings or epsilon.
    // CPU and CUDA consume the exact same admitted Compute IR.
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