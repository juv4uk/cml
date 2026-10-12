//! Integrity proof: optimizer witnesses must be replayed against the SAME
//! original physical canonical T5, rather than mutable AST/IR metadata.
//! Actual semantic oracle parity is separately tested in the O1 rewrite
//! corpus: deterministic replay alone is never claimed a mathematical proof.
use cml_current_sens_transport::current_exact_optimizer::{
    optimize_canonical_t5, replay_optimizer_witness_from_t5, OptimizePipelineError,
};

fn canonical(source: &str) -> Vec<u8> {
    sens::encode_binary_projection_ternary(source)
        .expect("exact D2/D3 source must encode canonically")
}

#[test]
fn both_rewrites_have_exact_physical_replay_and_reject_cross_program_witnesses() {
    let head = canonical("10 100 10 111 10 001 0 01 10 001 1 01 01 01");
    let tail = canonical("10 011 10 111 10 001 0 01 10 001 1 01 01 01");
    let first = optimize_canonical_t5(&head).unwrap();
    let second = optimize_canonical_t5(&tail).unwrap();
    assert_eq!(first.witnesses.len(), 1);
    assert_eq!(second.witnesses.len(), 1);
    assert_eq!(first.original_nodes, 4);
    assert_eq!(first.optimized_nodes, 1);
    assert!(replay_optimizer_witness_from_t5(&head, &first).unwrap());
    assert!(replay_optimizer_witness_from_t5(&tail, &second).unwrap());
    assert!(!replay_optimizer_witness_from_t5(&head, &second).unwrap());
    assert!(!replay_optimizer_witness_from_t5(&tail, &first).unwrap());

    let mut digest = first.clone();
    digest.canonical_t5_sha256[0] ^= 1;
    assert!(!replay_optimizer_witness_from_t5(&head, &digest).unwrap());

    let mut witness = first.clone();
    witness.witnesses[0].authority_sha256 = "0".repeat(64);
    assert!(!replay_optimizer_witness_from_t5(&head, &witness).unwrap());

    let mut cost = first.clone();
    cost.optimized_nodes += 1;
    assert!(!replay_optimizer_witness_from_t5(&head, &cost).unwrap());

    let mut changed_ir = first.clone();
    changed_ir.ir = second.ir.clone();
    assert!(!replay_optimizer_witness_from_t5(&head, &changed_ir).unwrap());
    println!("CML_SENS_OPTIMIZER_PHYSICAL_REPLAY_PASS cross_program_and_forged_witness_rejected=true");
}

#[test]
fn non_rewriting_quote_is_also_physically_attested_and_mutant_sources_fail_closed() {
    let quote = [0x63, 0x89, 0x06, 0xa1];
    let result = optimize_canonical_t5(&quote).unwrap();
    assert!(result.witnesses.is_empty());
    assert!(replay_optimizer_witness_from_t5(&quote, &result).unwrap());
    for candidate in [
        vec![], vec![0xf3,0x40,0x60], vec![0x63,0x89,0x06], vec![0xff; 7]
    ] {
        assert!(matches!(
            replay_optimizer_witness_from_t5(&candidate, &result),
            Err(OptimizePipelineError::InvalidPhysicalOrSemanticSource(_))
        ), "invalid source accepted as physical T5: {candidate:02x?}");
    }
    println!("CML_SENS_OPTIMIZER_PHYSICAL_REPLAY_FAIL_CLOSED_PASS invalid_carriers=4");
}
