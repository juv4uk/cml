#![cfg(feature = "gpu-cuda")]

use cml::gpu_cuda_runtime::execute_map;
use cml::ir::BufferLiteral;
use cml::{lower, parser};

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn admitted_i32_map_executes_on_cuda_device_zero() {
    let source = "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))";
    let expressions = parser::parse(source).unwrap();
    let ir = lower::lower_expr(&expressions[0]).unwrap();
    let execution = execute_map(&ir, 0).expect("live CUDA execution failed");
    assert_eq!(execution.device_ordinal, 0);
    assert_eq!(execution.output, BufferLiteral::I32(vec![2, 3, 4]));
}
