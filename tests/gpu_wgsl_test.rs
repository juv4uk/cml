use cml::compute::AdmissionBlocker;
use cml::gpu_wgsl::{WgslError, emit_map_shader};
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
fn dormant_affine_f32_ir_is_flattened_to_one_binary32_add() {
    // Source admission для F32 лишається закритим; emitter тут отримує IR
    // напряму, щоб окремо зберегти доказ своєї внутрішньої арифметики.
    let shader = emit_map_shader(&f32_map_ir(
        &[1.0, 2.0],
        Ir::Prim {
            op: PrimOp::Add,
            args: vec![
                Ir::Prim {
                    op: PrimOp::Add,
                    args: vec![Ir::Var("X".to_string()), Ir::Int(10)],
                },
                Ir::Int(-3),
            ],
        },
    ))
    .unwrap();
    assert!(shader.contains("array<f32>"));
    assert!(shader.contains("output_data[i] = x + 7.0;"));
    assert!(!shader.contains("x + 10"));
}

#[test]
fn emitter_rejects_non_affine_f32_ir() {
    let error = emit_map_shader(&f32_map_ir(
        &[1.0],
        Ir::Prim {
            op: PrimOp::Add,
            args: vec![Ir::Var("X".to_string()), Ir::Var("X".to_string())],
        },
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        WgslError::NotEligible(blockers)
            if blockers.contains(&AdmissionBlocker::FloatRoundingNotDefined)
    ));
}
