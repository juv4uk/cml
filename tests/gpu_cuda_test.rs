use cml::compute::{AdmissionBlocker, NumericDomain};
use cml::gpu_cuda::{CudaElementType, CudaEmitError, emit_map_kernel, lower_map_kernel};
use cml::ir::{BufferLiteral, Ir, Params};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

fn f32_map_ir(values: &[f32], body: Ir) -> Ir {
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["X".to_string()]),
                body: Box::new(body),
            },
            Ir::Buffer(BufferLiteral::F32(
                values.iter().map(|value| value.to_bits()).collect(),
            )),
        ],
    }
}

#[test]
fn admitted_i32_map_lowers_to_typed_cuda_artifact() {
    let artifact = lower_map_kernel(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
    ))
    .unwrap();

    assert_eq!(artifact.identity, sens::sens!(01011001));
    assert_eq!(artifact.numeric_domain, NumericDomain::FixedWidthInteger);
    assert_eq!(artifact.element_type, CudaElementType::I32);
    assert_eq!(artifact.parameter_count, 1);
    assert_eq!(artifact.entry_point, "cml_map");
    assert!(
        artifact
            .source
            .contains("extern \"C\" __global__ void cml_map")
    );
    assert!(artifact.source.contains("const int *input_data"));
    assert!(artifact.source.contains("if (i >= length) return;"));
    assert!(artifact.source.contains("output_data[i] = (x + 1);"));
}

#[test]
fn compatibility_emitter_preserves_lowered_source() {
    let ir = lower_one("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))");
    assert_eq!(
        emit_map_kernel(&ir).unwrap(),
        lower_map_kernel(&ir).unwrap().source
    );
}

#[test]
fn dormant_f32_ir_emits_one_binary32_add() {
    // Source-level F32 зараз не admitted. Явний IR не послаблює цю межу,
    // а лише зберігає перевірку вже наявного CUDA emitter-а.
    let source = emit_map_kernel(&f32_map_ir(
        &[1.0, 2.0],
        Ir::App {
            func: Box::new(Ir::Sid(sens::sens!(00001100))),
            args: vec![Ir::Var("X".to_string()), Ir::Int(3)],
        },
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
