use cml::compute::{
    AdmissionBlocker, BulkOperation, EffectClass, ExecutionShape, NumericDomain, StorageClass,
    analyze, refine_representation,
};
use cml::lower;
use cml::parser;

fn lower_one(source: &str) -> cml::ir::Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
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
    let analysis = analyze(&lower_one("(map (lambda (x) (cond (t x))) #i32(1 2 3))"));
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
    let analysis = analyze(&lower_one("(map (lambda (x) (+ x offset)) #i32(1 2))"));
    assert!(
        analysis
            .gpu_blockers
            .contains(&AdmissionBlocker::KernelNotLowerable)
    );
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
fn first_class_builtin_add_inside_map_matches_structural_primitive_form() {
    let source = parser::parse("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))").unwrap();
    let program = lower::lower_program_with_first_class_builtins(&source).unwrap();
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
