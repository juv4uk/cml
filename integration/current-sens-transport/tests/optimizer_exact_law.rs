//! Candidate differential tests for a SENS-specific optimizer, never SBCL
//! semantics. All actual executions go through SENS's canonical T5 oracle.
use cml_current_sens_transport::{
    current_exact_model::{CmlMechanism, ExactQuotedData, ModernIr},
    current_exact_optimizer::{
        optimize_exact_ir, replay_optimizer_witness, OptimizeError, RewriteLaw,
    },
    modern_ir::lower_verified_program,
    VerifiedPhysicalProgram,
};

fn compile_exact_source(source: &str)
    -> (Vec<u8>, sens::EvalResult, cml_current_sens_transport::current_exact_model::VerifiedProgramIr)
{
    let physical = sens::encode_binary_projection_ternary(source)
        .expect("ratified exact D2 + D3 source");
    let admitted = VerifiedPhysicalProgram::from_t5(&physical)
        .expect("no noncanonical source or guessed identity");
    let ir = lower_verified_program(&admitted).expect("only verified callable heads");
    let execution = sens::PhysicalT5Program::decode(&physical)
        .unwrap().execute(&mut sens::Session::default()).unwrap();
    (physical, execution, ir)
}

#[test]
fn both_verified_selector_laws_reduce_pure_pairs_and_match_the_sens_oracle() {
    for &(selector, law, selected) in &[
        ("100", RewriteLaw::QuotedHeadOfFreshPair, "0"),
        ("011", RewriteLaw::QuotedTailOfFreshPair, "1"),
    ] {
        // Literal bits are the already-ratified D3 exact callable heads.
        // Their SEMANTIC ROLES are still admitted only by the SENS compiler
        // law; the optimizer itself has no number -> meaning lookup.
        let source = format!("10 {selector} 10 111 10 001 0 01 10 001 1 01 01 01");
        let (t5, oracle, source_ir) = compile_exact_source(&source);
        let candidate = optimize_exact_ir(&source_ir).expect("sound strict rewrite");
        assert_eq!(candidate.witnesses.len(), 1, "one checked selector rewrite");
        assert_eq!(candidate.witnesses[0].law, law);
        assert_eq!(candidate.original_nodes, 4);
        assert_eq!(candidate.optimized_nodes, 1);
        assert!(replay_optimizer_witness(&source_ir, &candidate).unwrap());
        assert_eq!(candidate.canonical_t5_sha256, sens::sha256_source(&t5));
        let independent_quote = format!("10 001 {selected} 01");
        let (_small_t5, expected, compiled_quote) = compile_exact_source(&independent_quote);
        assert_eq!(oracle.value, expected.value);
        assert_eq!(oracle.output, expected.output);
        assert_eq!(candidate.ir, compiled_quote.ir);
        println!(
            "CML_SENS_OPTIMIZER_ORACLE_PASS law={law:?} before=4 after=1 exact_sha=true"
        );
    }
}

#[test]
fn nested_rewrite_saves_nodes_without_ever_changing_canonical_source() {
    let source = "10 100 10 111 10 001 0 01 10 001 1 01 01 01";
    let (t5, oracle, lower) = compile_exact_source(source);
    let optimized = optimize_exact_ir(&lower).unwrap();
    assert!(optimized.original_nodes > optimized.optimized_nodes);
    assert_eq!(optimized.witnesses.len(), 1);
    assert_eq!(optimized.canonical_t5_sha256, sens::sha256_source(&t5));
    // Only the optimizer's typed IR has changed. The ORIGINAL physical T5
    // is neither overwritten nor silently changed to a third codec.
    let unchanged = sens::PhysicalT5Program::decode(&t5)
        .unwrap().execute(&mut sens::Session::default()).unwrap();
    assert_eq!(oracle.value, unchanged.value);
    assert_eq!(oracle.output, unchanged.output);
}

#[test]
fn unsafe_eager_evaluation_elision_is_not_permitted() {
    let (_, _, mut source_ir) =
        compile_exact_source("10 100 10 111 10 001 0 01 10 001 1 01 01 01");
    let ModernIr::Mechanism { args, .. } = &mut source_ir.ir[0] else {
        panic!("selector expected");
    };
    let ModernIr::Mechanism { args: pair_args, .. } = &mut args[0] else {
        panic!("pair expected");
    };
    // A non-QUOTE operand may fail or have an effect; even an apparently
    // innocent literal is NOT whitelisted as a pure, total expression.
    pair_args[1] = ModernIr::Literal(ExactQuotedData::EmptyList);
    let report = optimize_exact_ir(&source_ir).unwrap();
    assert_eq!(report.witnesses.len(), 0);
    assert_eq!(report.original_nodes, report.optimized_nodes);
    assert_eq!(report.ir, source_ir.ir);

    // Forged target mechanisms must be rejected even if the carried SENS
    // exact domain identity has a genuine but DIFFERENT admitted role.
    let ModernIr::Mechanism { target, .. } = &mut source_ir.ir[0] else {
        panic!("selector expected");
    };
    *target = CmlMechanism::PairConstruct;
    assert_eq!(
        optimize_exact_ir(&source_ir),
        Err(OptimizeError::RoleTargetMismatch)
    );
}

#[test]
fn quote_data_is_not_executed_as_an_optimization_side_effect() {
    let source = "10 001 10 10 10 0 01 01 01 01";
    let (_t5, _, ir) = compile_exact_source(source);
    let report = optimize_exact_ir(&ir).unwrap();
    assert_eq!(report.witnesses.len(), 0);
    assert_eq!(report.ir, ir.ir);
    assert!(replay_optimizer_witness(&ir, &report).unwrap());
}

#[test]
fn forged_law_or_digest_is_rejected_before_optimization() {
    let (_, _, mut source_ir) =
        compile_exact_source("10 100 10 111 10 001 0 01 10 001 1 01 01 01");
    let ModernIr::Mechanism { binding, .. } = &mut source_ir.ir[0] else {
        panic!("selector expected");
    };
    binding.authority_sha256 = "0".repeat(64);
    assert_eq!(optimize_exact_ir(&source_ir), Err(OptimizeError::UnprovedRole));
}

#[test]
fn optimizer_is_conservative_on_the_real_tiny_quote_carrier() {
    let quote_t5 = [0x63, 0x89, 0x06, 0xa1];
    let source = VerifiedPhysicalProgram::from_t5(&quote_t5).unwrap();
    let ir = lower_verified_program(&source).unwrap();
    let result = optimize_exact_ir(&ir).unwrap();
    assert!(result.witnesses.is_empty());
    assert_eq!(result.original_nodes, result.optimized_nodes);
    assert_eq!(result.ir, ir.ir);
    assert!(replay_optimizer_witness(&ir, &result).unwrap());
}
