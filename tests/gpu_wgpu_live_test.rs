#![cfg(feature = "gpu-wgpu")]

use cml::gpu_wgpu_runtime::execute_map_blocking;
use cml::ir::BufferLiteral;
use cml::{lower, parser};

#[test]
#[ignore = "requires a live wgpu adapter; run explicitly to produce hardware evidence"]
fn admitted_i32_map_executes_on_live_adapter() {
    let source = "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))";
    let expressions = parser::parse(source).unwrap();
    let ir = lower::lower_expr(&expressions[0]).unwrap();
    let execution = execute_map_blocking(&ir).expect("live wgpu execution failed");

    eprintln!("adapter evidence: {:?}", execution.adapter);
    assert_eq!(execution.output, BufferLiteral::I32(vec![2, 3, 4]));
}
