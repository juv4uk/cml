use cml::compute::AdmissionBlocker;
use cml::gpu_wgsl::{WgslError, emit_map_shader};
use cml::{lower, parser};

fn lower_one(source: &str) -> cml::ir::Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

// F32 source is globally rejected today, but the lower-level IR/emitter still
// has useful fail-closed behavior worth testing independently.
fn lower_internal_one(source: &str) -> cml::ir::Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_expr(&expressions[0]).unwrap()
}

#[test]
fn emits_portable_i32_map_shader_from_admitted_ir() {
    let shader = emit_map_shader(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
    ))
    .unwrap();
    assert!(shader.contains("array<i32>"));
    assert!(shader.contains("@compute @workgroup_size(64)"));
    assert!(shader.contains("arrayLength(&input_data)"));
    assert!(shader.contains("output_data[i] = (x + 1i);"));
}

#[test]
fn internal_affine_f32_ir_is_flattened_to_one_binary32_add() {
    let shader = emit_map_shader(&lower_internal_one(
        "(numeric-buffer-map (lambda (x) (+ (+ x 10) -3)) #f32(1.0 2.0))",
    ))
    .unwrap();
    assert!(shader.contains("array<f32>"));
    assert!(shader.contains("output_data[i] = x + 7.0;"));
    assert!(!shader.contains("x + 10"));
}

#[test]
fn emitter_rejects_non_affine_f32_ir() {
    let error = emit_map_shader(&lower_internal_one(
        "(numeric-buffer-map (lambda (x) (+ x x)) #f32(1.0))",
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        WgslError::NotEligible(blockers)
            if blockers.contains(&AdmissionBlocker::FloatRoundingNotDefined)
    ));
}
