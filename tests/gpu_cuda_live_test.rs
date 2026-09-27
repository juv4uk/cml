#![cfg(feature = "gpu-cuda")]

use cml::accelerator::{select_accelerator, AcceleratorApi, AcceleratorVendor, SelectionPolicy};
use cml::gpu_cuda_runtime::{discover_devices, execute_map, CudaSession};
use cml::ir::BufferLiteral;
use cml::{lower, parser};

fn lower_one(source: &str) -> cml::ir::Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_expr(&expressions[0]).unwrap()
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
