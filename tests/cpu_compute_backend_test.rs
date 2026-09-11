use cml::compute::{AdmissionBlocker, ComputeBackend, ComputeExecutionError, CpuComputeBackend};
use cml::ir::{BufferLiteral, Ir};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

#[test]
fn cpu_reference_executes_the_canonical_i32_map() {
    let result = CpuComputeBackend
        .execute(&lower_one(
            "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
        ))
        .unwrap();
    assert_eq!(result, BufferLiteral::I32(vec![2, 3, 4]));
}

#[test]
fn cpu_reference_preserves_empty_and_negative_buffers() {
    let empty = CpuComputeBackend
        .execute(&lower_one(
            "(numeric-buffer-map (lambda (x) (+ x 1)) #i32())",
        ))
        .unwrap();
    assert_eq!(empty, BufferLiteral::I32(vec![]));

    let negative = CpuComputeBackend
        .execute(&lower_one(
            "(numeric-buffer-map (lambda (x) (+ x -2)) #i32(-3 4))",
        ))
        .unwrap();
    assert_eq!(negative, BufferLiteral::I32(vec![-5, 2]));
}

#[test]
fn cpu_reference_refuses_unproven_overflow_and_semantics_refuse_f32() {
    let overflow = CpuComputeBackend
        .execute(&lower_one(
            "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(2147483647))",
        ))
        .unwrap_err();
    assert!(matches!(
        overflow,
        ComputeExecutionError::NotEligible(blockers)
            if blockers.contains(&AdmissionBlocker::IntegerOverflowNotProven)
    ));

    // F32 зараз відсікається раніше за compute backend: це глобальна
    // fail-closed межа семантичного lowering, а не локальна політика CPU.
    let expressions = parser::parse(
        "(numeric-buffer-map (lambda (x) (+ x x)) #f32(1.0))",
    )
    .unwrap();
    let float = lower::lower_program(&expressions)
        .expect_err("F32 buffer must be rejected before compute-backend admission");
    assert!(
        float.to_string().contains("UnsupportedF32Buffer"),
        "unexpected F32 rejection: {float}"
    );
}
