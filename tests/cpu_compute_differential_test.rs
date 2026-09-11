use cml::compute::{AdmissionBlocker, ComputeBackend, ComputeExecutionError, CpuComputeBackend};
use cml::ir::{BufferLiteral, Ir};
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

fn lower_internal_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_expr(&expressions[0]).unwrap()
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
fn internal_f32_cpu_compute_matches_oracle_below_source_admission_gate() {
    // CML source admission зараз fail-closed для #f32(...), тому цей тест
    // перевіряє лише вже наявний нижчий IR/CPU механізм, не заявляючи підтримку source path.
    for source in [
        "(numeric-buffer-map (lambda (x) (+ x 1)) #f32(1.0 -2.5 0.1))",
        "(numeric-buffer-map (lambda (x) (+ (+ x 10) -3)) #f32(1.0 -2.5 0.1))",
    ] {
        assert_eq!(
            execute(&lower_internal_one(source)),
            oracle(source),
            "source: {source}"
        );
    }
}
