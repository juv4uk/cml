use cml::compute::AdmissionBlocker;
use cml::gpu_cuda::{CudaEmitError, emit_map_kernel};
use cml::{lower, parser};

fn lower_one(source: &str) -> cml::ir::Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_expr(&expressions[0]).unwrap()
}

#[test]
fn admitted_i32_map_emits_bounds_checked_cuda_kernel() {
    let source = emit_map_kernel(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
    ))
    .unwrap();
    assert!(source.contains("extern \"C\" __global__ void cml_map"));
    assert!(source.contains("const int *input_data"));
    assert!(source.contains("if (i >= length) return;"));
    assert!(source.contains("output_data[i] = (x + 1);"));
}

#[test]
fn admitted_f32_map_emits_one_binary32_add() {
    let source = emit_map_kernel(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ (+ x 1) 2)) #f32(1.0 2.0))",
    ))
    .unwrap();
    assert!(source.contains("const float *input_data"));
    assert!(source.contains("output_data[i] = x + 3.0f;"));
    assert!(!source.contains("(x + 1) + 2"));
}

#[test]
fn cuda_emitter_cannot_bypass_semantic_admission() {
    let error = emit_map_kernel(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(2147483647))",
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        CudaEmitError::NotEligible(blockers)
            if blockers.contains(&AdmissionBlocker::IntegerOverflowNotProven)
    ));
}
