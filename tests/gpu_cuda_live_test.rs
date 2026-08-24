#![cfg(feature = "gpu-cuda")]

use cml::accelerator::{AcceleratorApi, AcceleratorVendor, SelectionPolicy, select_accelerator};
use cml::gpu_cuda_runtime::{discover_devices, execute_map};
use cml::ir::BufferLiteral;
use cml::{lower, parser};

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

    let source = "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))";
    let expressions = parser::parse(source).unwrap();
    let ir = lower::lower_expr(&expressions[0]).unwrap();
    let execution = execute_map(&ir, 0).expect("live CUDA execution failed");
    eprintln!("CUDA device evidence: {:?}", execution.device);
    assert_eq!(execution.device.ordinal, 0);
    assert_eq!(execution.device.descriptor, *selected);
    assert_eq!(execution.output, BufferLiteral::I32(vec![2, 3, 4]));
}
