use cml::compute::{AdmissionBlocker, ComputeBackend, ComputeExecutionError, CpuComputeBackend};
use cml::ir::{BufferLiteral, Ir, Params, PrimOp};
use cml::{lower, parser};
use my_lisp::{ErrorKind, Session, eval_program};

#[derive(Debug, PartialEq, Eq)]
enum Observable {
    Value(String),
    Error(ErrorKind),
}

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

fn oracle(source: &str) -> Observable {
    match eval_program(source, &mut Session::default()) {
        Ok(result) => Observable::Value(result.value.to_string()),
        Err(error) => Observable::Error(error.kind),
    }
}

fn render_buffer(buffer: BufferLiteral) -> String {
    match buffer {
        BufferLiteral::I32(values) => format!(
            "#i32({})",
            values
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        ),
        BufferLiteral::F32(values) => format!(
            "#f32({})",
            values
                .iter()
                .map(|bits| {
                    let value = f32::from_bits(*bits);
                    if value.fract() == 0.0 {
                        format!("{value:.1}")
                    } else {
                        value.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

fn execute(ir: &Ir) -> Observable {
    match CpuComputeBackend.execute(ir) {
        Ok(buffer) => Observable::Value(render_buffer(buffer)),
        Err(ComputeExecutionError::NotEligible(blockers))
            if blockers.contains(&AdmissionBlocker::IntegerOverflowNotProven) =>
        {
            Observable::Error(ErrorKind::NumericOverflow)
        }
        Err(error) => panic!("unexpected CPU ComputeBackend outcome: {error:?}"),
    }
}

#[test]
fn admitted_i32_cpu_compute_matches_the_live_canonical_evaluator() {
    for source in [
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
        "(numeric-buffer-map (lambda (x) (+ x -2)) #i32(-3 4))",
        "(numeric-buffer-map (lambda (x) (+ (+ x 10) -3)) #i32(0 7 -9))",
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32())",
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(2147483647))",
    ] {
        assert_eq!(
            execute(&lower_one(source)),
            oracle(source),
            "source: {source}"
        );
    }
}

#[test]
fn dormant_f32_cpu_ir_matches_the_live_canonical_evaluator() {
    // Source admission у CML зараз fail-closed для #f32(...). Тут ми не
    // обходимо цю межу: будуємо нижчий IR явно і звіряємо вже наявний CPU
    // механізм із канонічним evaluator-ом my-lisp.
    let cases = [
        (
            "(numeric-buffer-map (lambda (x) (+ x 1)) #f32(1.0 -2.5 0.1))",
            f32_map_ir(
                &[1.0, -2.5, 0.1],
                Ir::Prim {
                    op: PrimOp::Add,
                    args: vec![Ir::Var("X".to_string()), Ir::Int(1)],
                },
            ),
        ),
        (
            "(numeric-buffer-map (lambda (x) (+ (+ x 10) -3)) #f32(1.0 -2.5 0.1))",
            f32_map_ir(
                &[1.0, -2.5, 0.1],
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
            ),
        ),
    ];

    for (source, ir) in cases {
        assert_eq!(execute(&ir), oracle(source), "source: {source}");
    }
}
