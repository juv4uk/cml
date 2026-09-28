use cml::compute::{
    AdmissionBlocker, BulkOperation, EffectClass, ExecutionShape, NumericDomain, StorageClass,
    analyze, refine_representation,
};
use cml::ir::Ir;
use cml::lower;
use cml::parser;

fn lower_one(source: &str) -> cml::ir::Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

#[test]
fn map_and_reduce_lower_to_exact_sid8_identity() {
    for (source, expected) in [
        ("(map (lambda (x) (+ x 1)) data)", sens::sid!(00110111)),
        (
            "(reduce (lambda (acc x) (+ acc x)) 0 data)",
            sens::sid!(00111001),
        ),
    ] {
        let ir = lower_one(source);
        let Ir::App { func, .. } = ir else {
            panic!("expected Sid8-keyed application, got {ir:?}");
        };
        assert!(
            matches!(func.as_ref(), Ir::Sid(sid) if *sid == expected),
            "expected exact Sid8 {expected}, got {func:?}"
        );
    }
}

#[test]
fn recognizes_map_without_pretending_a_list_is_a_gpu_buffer() {
    let analysis = analyze(&lower_one("(map (lambda (x) (+ x 1)) (quote (1 2 3)))"));
    assert_eq!(analysis.shape, ExecutionShape::ElementWise);
    assert_eq!(analysis.effect, EffectClass::Pure);
    assert_eq!(analysis.storage, StorageClass::LinkedList);
    assert_eq!(analysis.numeric_domain, NumericDomain::Exact);
    assert_eq!(analysis.region.unwrap().operation, BulkOperation::Map);
    assert!(
        analysis
            .gpu_blockers
            .contains(&AdmissionBlocker::StorageNotContiguous)
    );
    assert!(
        analysis
            .gpu_blockers
            .contains(&AdmissionBlocker::NumericDomainNotRepresentable)
    );
}

#[test]
fn pure_but_unsupported_kernel_shape_stays_fail_closed() {
    // `numeric-buffer-map` (01011001) is the exact identity for mapping a
    // numeric buffer. The list `map` (00110111) must not stand in for it.
    let analysis = analyze(&lower_one(
        "(numeric-buffer-map (lambda (x) (cond (t x))) #i32(1 2 3))",
    ));
    assert_eq!(analysis.effect, EffectClass::Pure);
    assert!(
        analysis
            .gpu_blockers
            .contains(&AdmissionBlocker::KernelNotLowerable)
    );
    assert!(!analysis.gpu_eligible());
}

#[test]
fn captured_values_are_not_mistaken_for_kernel_parameters() {
    let analysis = analyze(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ x offset)) #i32(1 2))",
    ));
    assert!(
        analysis
            .gpu_blockers
            .contains(&AdmissionBlocker::KernelNotLowerable)
    );
}

#[test]
fn list_map_and_numeric_buffer_map_stay_distinct_identities() {
    // cml#344: `00110111` (list map) and `01011001` (numeric-buffer-map) are
    // two different functions, not two names for one thing. A numeric buffer
    // must fail closed under the list-map identity instead of riding the
    // numeric-buffer compute route.
    let list_map = analyze(&lower_one("(map (lambda (x) (+ x 1)) (quote (1 2 3)))"));
    let list_region = list_map
        .region
        .as_ref()
        .expect("list map has explicit list evidence, so it keeps its own admitted path");
    assert_eq!(list_region.identity, sens::sens!(00110111));
    assert_eq!(list_region.operation, BulkOperation::Map);

    let numeric = analyze(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
    ));
    let numeric_region = numeric
        .region
        .as_ref()
        .expect("numeric-buffer-map is admitted for a numeric buffer");
    assert_eq!(numeric_region.identity, sens::sens!(01011001));
    assert_eq!(numeric_region.operation, BulkOperation::Map);

    assert_ne!(
        list_region.identity, numeric_region.identity,
        "the two exact identities must never collapse into one"
    );

    // The forbidden dual-identity path: numeric buffer under the list map.
    let crossed = analyze(&lower_one("(map (lambda (x) (+ x 1)) #i32(1 2 3))"));
    assert!(
        crossed.region.is_none(),
        "a numeric buffer must not be admitted as a bulk region under list-map identity \
         00110111 (cml#344)"
    );
    assert!(!crossed.gpu_eligible());
}

#[test]
fn recognizes_reduce_as_a_distinct_execution_shape() {
    let analysis = analyze(&lower_one(
        "(reduce (lambda (acc x) (+ acc x)) 0 (quote (1 2 3 4)))",
    ));
    assert_eq!(analysis.shape, ExecutionShape::Reduction);
    assert_eq!(analysis.region.unwrap().operation, BulkOperation::Reduce);
}

#[test]
fn sid8_cons_is_classified_as_allocating() {
    let analysis = analyze(&lower_one("(cons 1 2)"));
    assert_eq!(analysis.effect, EffectClass::Allocating);
    assert!(!analysis.gpu_eligible());
}

#[test]
fn generic_calls_are_not_assumed_pure() {
    let analysis = analyze(&lower_one("(mystery 1 2)"));
    assert_eq!(analysis.shape, ExecutionShape::Irregular);
    assert_eq!(analysis.effect, EffectClass::Unknown);
    assert!(!analysis.gpu_eligible());
}

#[test]
fn unknown_calls_inside_a_map_kernel_block_gpu_admission() {
    let mut analysis = analyze(&lower_one("(map (lambda (x) (mystery x)) data)"));
    refine_representation(
        &mut analysis,
        StorageClass::ContiguousBuffer,
        NumericDomain::FixedWidthInteger,
    );
    assert_eq!(analysis.effect, EffectClass::Unknown);
    assert!(
        analysis
            .gpu_blockers
            .contains(&AdmissionBlocker::EffectNotPure)
    );
}

#[test]
fn proven_fixed_width_contiguous_representation_unlocks_gpu_candidate() {
    let mut analysis = analyze(&lower_one("(map (lambda (x) (+ x 1)) data)"));
    assert!(!analysis.gpu_eligible());
    refine_representation(
        &mut analysis,
        StorageClass::ContiguousBuffer,
        NumericDomain::FixedWidthInteger,
    );
    assert!(!analysis.gpu_eligible());
    assert!(
        analysis
            .gpu_blockers
            .contains(&AdmissionBlocker::IntegerOverflowNotProven)
    );
}

#[test]
fn sid8_add_inside_numeric_buffer_map_is_gpu_eligible() {
    let source = parser::parse("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))").unwrap();
    let program = lower::lower_program(&source).unwrap();
    let analysis = analyze(&program[0]);
    assert!(
        analysis.gpu_eligible(),
        "unexpected blockers: {:?}",
        analysis.gpu_blockers
    );
    assert_eq!(analysis.numeric_domain, NumericDomain::FixedWidthInteger);
}

#[test]
fn exact_numbers_are_never_silently_refined_to_float() {
    let mut analysis = analyze(&lower_one("(map (lambda (x) (+ x 1)) (quote (1 2 3)))"));
    refine_representation(
        &mut analysis,
        StorageClass::ContiguousBuffer,
        NumericDomain::Exact,
    );
    assert!(!analysis.gpu_eligible());
    assert!(
        analysis
            .gpu_blockers
            .contains(&AdmissionBlocker::NumericDomainNotRepresentable)
    );
}
