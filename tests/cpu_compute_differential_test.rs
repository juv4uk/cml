use cml::compute::{
    AdmissionBlocker, ComputeBackend, ComputeExecutionError, CpuComputeBackend,
};
use cml::ir::{BufferLiteral, Ir};
use cml::{lower, parser};
use my_lisp::{eval_program, ErrorKind, Session};

#[derive(Debug, PartialEq, Eq)]
enum Observable {
    Value(String),
    Error(ErrorKind),
}

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
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
        BufferLiteral::F32(_) => unreachable!("f32 is not admitted by CPU M0"),
    }
}

fn compiled(source: &str) -> Observable {
    match CpuComputeBackend.execute(&lower_one(source)) {
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
fn cpu_compute_matches_the_live_canonical_evaluator() {
    for source in [
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
        "(numeric-buffer-map (lambda (x) (+ x -2)) #i32(-3 4))",
        "(numeric-buffer-map (lambda (x) (+ (+ x 10) -3)) #i32(0 7 -9))",
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32())",
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(2147483647))",
    ] {
        assert_eq!(compiled(source), oracle(source), "source: {source}");
    }
}
