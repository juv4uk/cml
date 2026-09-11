use cml::compute::AdmissionBlocker;
use cml::gpu_cuda::{CudaEmitError, emit_map_kernel};
use cml::ir::{BufferLiteral, Ir, Params, PrimOp};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

fn f32_map_ir(values: &[f32], body: Ir) -> Ir {
    Ir::App {
        func: Box::new(Ir::Builtin("NUMERIC-BUFFER-MAP".to_string())),
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
fn dormant_f32_ir_emits_one_binary32_add() {
    // Source-level F32 зараз не admitted. Явний IR не послаблює цю межу,
    // а лише зберігає перевірку вже наявного CUDA emitter-а.
    let source = emit_map_kernel(&f32_map_ir(
        &[1.0, 2.0],
        Ir::Prim {
            op: PrimOp::Add,
            args: vec![
                Ir::Prim {
                    op: PrimOp::Add,
                    args: vec![Ir::Var("X".to_string()), Ir::Int(1)],
                },
                Ir::Int(2),
            ],
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
