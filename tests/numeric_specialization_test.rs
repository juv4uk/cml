//! Conformance, differential witnesses, and semantic parity tests for
//! Proof-Driven Numeric Specialization (#55).
//!
//! Verifies:
//! 1. Multi-operation arithmetic (`(+ (+ 10 20) 12) -> 42`, `(- (+ 50 20) 28) -> 42`)
//!    chains across virtual registers completely unboxed without intermediate tagging.
//! 2. Semantic parity is strictly judged against the upstream `my-lisp` evaluation oracle.
//! 3. Boundary values remain semantically identical between specialized and canonical modes.
//! 4. Raw machine contract (`BoundaryConvention::RawU64`) has ZERO box and ZERO unbox operations.
//! 5. Disabling specialization produces the identical numeric outcome with canonical per-op boxing.
//! 6. Out-of-domain / potential overflow arithmetic does NOT enter the unboxed path.
//! 7. Inspectable and deterministic dump of analysis facts and provenance.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::machine_inst::assemble_program;
use cml::native_baseline::NativeExecutable;
use cml::numeric_specialization::{
    BoundaryConvention, MAX_FIXNUM, NumericDomain, OverflowProof, SpecializationMode,
    count_boxing_insts, lower_ir_to_specialized_lir,
};
use cml::x86_lir::lir_to_machine_items;
use cml::{lower, parser};
use my_lisp::{Session, eval_program};
use wsm_os_target::Tag;

fn parse_and_lower_ir(source: &str) -> cml::ir::Ir {
    let exprs =
        parser::parse(source).unwrap_or_else(|e| panic!("parse error for `{source}`: {e:?}"));
    let mut lowered = lower::lower_program(&exprs)
        .unwrap_or_else(|e| panic!("lowering error for `{source}`: {e}"));
    assert_eq!(lowered.len(), 1);
    lowered.remove(0)
}

fn lisp_oracle(source: &str) -> u64 {
    let mut session = Session::default();
    eval_program(source, &mut session)
        .unwrap_or_else(|e| panic!("oracle eval error for `{source}`: {e:?}"))
        .value
        .to_string()
        .parse::<u64>()
        .unwrap_or_else(|e| panic!("oracle returned non-u64 for `{source}`: {e}"))
}

#[test]
fn test_chained_unboxed_arithmetic_witness_native_parity() {
    let source = "(+ (+ 10 20) 12)";
    let ir = parse_and_lower_ir(source);

    // 1. Lower with numeric specialization enabled under dynamic Lisp fixnum boundary
    let (lir_func, analysis) = lower_ir_to_specialized_lir(
        &ir,
        BoundaryConvention::BoxedFixnum,
        SpecializationMode::Enabled,
    )
    .expect("lower with specialization");

    // Acceptance criterion 1: multi-operation arithmetic stays unboxed across operations
    assert_eq!(
        analysis.unboxed_alu_count, 2,
        "both additions must be executed in unboxed raw registers"
    );
    assert!(
        analysis.eliminated_box_count >= 3,
        "intermediate constant and ALU box operations must be eliminated"
    );

    // Structural check: only 1 box instruction (at function exit boundary), 0 unboxes
    let (box_count, unbox_count) = count_boxing_insts(&lir_func);
    assert_eq!(
        box_count, 1,
        "specialized Lisp function must box exactly once at the exit boundary"
    );
    assert_eq!(
        unbox_count, 0,
        "specialized function on constants requires 0 unbox operations"
    );

    // 2. Emit machine items and execute natively
    let machine_items = lir_to_machine_items(&lir_func).expect("emit machine items");
    let bytes = assemble_program(&machine_items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    let native_result = exec.call();

    // Acceptance criterion 2: judge semantic parity against the upstream Lisp oracle
    let oracle_val = lisp_oracle(source);
    let expected_tagged = (oracle_val << 3) | (Tag::Fixnum as u64);

    assert_eq!(
        native_result, expected_tagged,
        "native return value must be the correctly tagged fixnum representation"
    );
    let unboxed_native = native_result >> 3;
    assert_eq!(
        unboxed_native, oracle_val,
        "unboxed native result must match Lisp oracle evaluation"
    );
}

#[test]
fn test_subtraction_chained_unboxed_arithmetic() {
    let source = "(- (+ 50 20) 28)";
    let ir = parse_and_lower_ir(source);

    let (lir_func, analysis) = lower_ir_to_specialized_lir(
        &ir,
        BoundaryConvention::BoxedFixnum,
        SpecializationMode::Enabled,
    )
    .expect("lower with specialization");

    assert_eq!(analysis.unboxed_alu_count, 2);

    let (box_count, unbox_count) = count_boxing_insts(&lir_func);
    assert_eq!(box_count, 1);
    assert_eq!(unbox_count, 0);

    let machine_items = lir_to_machine_items(&lir_func).expect("emit machine items");
    let bytes = assemble_program(&machine_items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    let native_result = exec.call();

    let oracle_val = lisp_oracle(source);
    let expected_tagged = (oracle_val << 3) | (Tag::Fixnum as u64);

    assert_eq!(native_result, expected_tagged);
    assert_eq!(native_result >> 3, oracle_val);
}

#[test]
fn test_raw_u64_boundary_has_zero_boxing_across_region() {
    let source = "(+ (+ 10 20) 12)";
    let ir = parse_and_lower_ir(source);

    // Bounded raw machine contract (e.g. my-lisp#118 x86-lower-add-u64)
    let (lir_func, analysis) =
        lower_ir_to_specialized_lir(&ir, BoundaryConvention::RawU64, SpecializationMode::Enabled)
            .expect("lower raw-u64");

    assert_eq!(analysis.unboxed_alu_count, 2);

    // Witness requirement: zero boxing in the entire region for raw contract
    let (box_count, unbox_count) = count_boxing_insts(&lir_func);
    assert_eq!(
        box_count, 0,
        "raw u64 contract must have zero box instructions"
    );
    assert_eq!(
        unbox_count, 0,
        "raw u64 contract must have zero unbox instructions"
    );

    let machine_items = lir_to_machine_items(&lir_func).expect("emit machine items");
    let bytes = assemble_program(&machine_items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    let native_result = exec.call();

    let oracle_val = lisp_oracle(source);
    assert_eq!(
        native_result, oracle_val,
        "raw u64 execution returns the raw unboxed integer directly in RAX"
    );
}

#[test]
fn test_disabling_specialization_produces_identical_lisp_outcome_with_canonical_boxing() {
    let source = "(+ 10 32)";
    let ir = parse_and_lower_ir(source);

    // 1. Lower with specialization DISABLED
    let (unspec_lir, unspec_analysis) = lower_ir_to_specialized_lir(
        &ir,
        BoundaryConvention::BoxedFixnum,
        SpecializationMode::Disabled,
    )
    .expect("lower disabled");

    assert_eq!(unspec_analysis.unboxed_alu_count, 0);
    assert_eq!(unspec_analysis.eliminated_box_count, 0);

    let (unspec_boxes, unspec_unboxes) = count_boxing_insts(&unspec_lir);
    assert!(
        unspec_boxes >= 2,
        "unspecialized path must box constants and result"
    );
    assert!(
        unspec_unboxes >= 2,
        "unspecialized path must unbox operands"
    );

    let unspec_items = lir_to_machine_items(&unspec_lir).expect("emit unspec items");
    let unspec_bytes = assemble_program(&unspec_items).expect("assemble unspec");
    let unspec_exec = NativeExecutable::load(&unspec_bytes);
    let unspec_result = unspec_exec.call();

    // 2. Lower with specialization ENABLED
    let (spec_lir, _spec_analysis) = lower_ir_to_specialized_lir(
        &ir,
        BoundaryConvention::BoxedFixnum,
        SpecializationMode::Enabled,
    )
    .expect("lower enabled");

    let (spec_boxes, spec_unboxes) = count_boxing_insts(&spec_lir);
    assert_eq!(spec_boxes, 1);
    assert_eq!(spec_unboxes, 0);

    let spec_items = lir_to_machine_items(&spec_lir).expect("emit spec items");
    let spec_bytes = assemble_program(&spec_items).expect("assemble spec");
    let spec_exec = NativeExecutable::load(&spec_bytes);
    let spec_result = spec_exec.call();

    // Parity check: both produce the EXACT same machine observable value
    let oracle_val = lisp_oracle(source);
    let expected_tagged = (oracle_val << 3) | (Tag::Fixnum as u64);

    assert_eq!(unspec_result, expected_tagged);
    assert_eq!(spec_result, expected_tagged);
    assert_eq!(unspec_result, spec_result);
}

#[test]
fn test_out_of_domain_or_overflow_does_not_enter_unboxed_path() {
    // Construct an addition that exceeds 61-bit fixnum capacity:
    // MAX_FIXNUM + 10 overflows the admitted fixnum domain
    let source = format!("(+ {} 10)", MAX_FIXNUM);
    let ir = parse_and_lower_ir(&source);

    let (_lir_func, analysis) = lower_ir_to_specialized_lir(
        &ir,
        BoundaryConvention::BoxedFixnum,
        SpecializationMode::Enabled,
    )
    .expect("lower overflow candidate");

    // Guard requirement: cannot specialize unproven overflow
    assert_eq!(
        analysis.unboxed_alu_count, 0,
        "potential overflow must not enter the unboxed raw path"
    );

    // Verify fact recorded for the result register
    let mut found_unknown = false;
    for fact in analysis.facts.values() {
        if fact.overflow == OverflowProof::Unknown {
            found_unknown = true;
            assert_eq!(fact.domain, NumericDomain::DynamicUnknown);
            break;
        }
    }
    assert!(
        found_unknown,
        "overflowing operation must record OverflowProof::Unknown and DynamicUnknown domain"
    );
}

#[test]
fn test_inspectable_deterministic_dump() {
    let source = "(+ 10 32)";
    let ir = parse_and_lower_ir(source);

    let (_lir_func, analysis) = lower_ir_to_specialized_lir(
        &ir,
        BoundaryConvention::BoxedFixnum,
        SpecializationMode::Enabled,
    )
    .expect("lower");

    let dump = analysis.dump();
    assert!(dump.contains("SpecializationAnalysis"));
    assert!(dump.contains("unboxed ALU ops: 1"));
    assert!(dump.contains("eliminated boxes:"));
    assert!(dump.contains("domain="));
    assert!(dump.contains("rep=unboxed-raw"));
    assert!(dump.contains("overflow=proven-no-overflow"));
    assert!(dump.contains("prov=numeric_specialization"));
}

#[test]
fn test_structural_metrics_comparison_specialized_vs_unspecialized() {
    let source = "(+ 10 32)";
    let ir = parse_and_lower_ir(source);

    let (unspec_lir, _) = lower_ir_to_specialized_lir(
        &ir,
        BoundaryConvention::BoxedFixnum,
        SpecializationMode::Disabled,
    )
    .expect("lower unspecialized");
    let unspec_items = lir_to_machine_items(&unspec_lir).expect("emit unspecialized items");
    let unspec_metrics = cml::native_baseline::StructuralMetrics::from_machine_items(&unspec_items);

    let (spec_lir, _) = lower_ir_to_specialized_lir(
        &ir,
        BoundaryConvention::BoxedFixnum,
        SpecializationMode::Enabled,
    )
    .expect("lower specialized");
    let spec_items = lir_to_machine_items(&spec_lir).expect("emit specialized items");
    let spec_metrics = cml::native_baseline::StructuralMetrics::from_machine_items(&spec_items);

    assert!(
        spec_metrics.instruction_count < unspec_metrics.instruction_count,
        "specialized code must have fewer instructions: got {} vs unspecialized {}",
        spec_metrics.instruction_count,
        unspec_metrics.instruction_count
    );
    assert!(
        spec_metrics.code_bytes < unspec_metrics.code_bytes,
        "specialized code must have smaller byte size: got {} vs unspecialized {}",
        spec_metrics.code_bytes,
        unspec_metrics.code_bytes
    );
}
