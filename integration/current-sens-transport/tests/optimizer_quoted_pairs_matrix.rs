//! Bounded exhaustive law check for SENS-specific, exact-domain O1 rewrites.
//! Unlike a host-language simplifier this compares actual SENS execution of
//! every source term with the independently encoded selected QUOTE term.
//! All role selection inside the optimizer comes from SENS's executable law;
//! the binary heads below are test vectors from the ratified D3 domain only.
//!
//! A green proof here is a source-level oracle differential, not proof of
//! native CML machine-code speed or whole-language equivalence.
use cml_current_sens_transport::{
    current_exact_optimizer::{optimize_canonical_t5, replay_optimizer_witness, RewriteLaw},
    current_exact_model::lower_canonical_t5_bytes,
};

fn physical(source: &str) -> Vec<u8> {
    sens::encode_binary_projection_ternary(source)
        .unwrap_or_else(|e| panic!("invalid canonical D2/D3 test vector {source:?}: {e:?}"))
}

fn execute(physical: &[u8]) -> sens::EvalResult {
    sens::PhysicalT5Program::decode(physical)
        .expect("the test must use canonical physical T5")
        .execute(&mut sens::Session::default())
        .expect("the test expression must be executable by SENS")
}

#[test]
fn all_quoted_pairs_of_bounded_bit_and_list_data_match_independent_sens_oracle() {
    // 0/1 D1 atoms, empty list, and a one-element proper list. The latter
    // is IMPORTANT: the optimizer must preserve a whole quoted list as data,
    // not try to execute its non-callable structural head.
    const EXACT_DATA: &[&str] = &["0", "1", "10 01", "10 0 01"];
    let mut checked = 0usize;
    for &(selector, rule, chosen_index) in &[
        ("100", RewriteLaw::QuotedHeadOfFreshPair, 0usize),
        ("011", RewriteLaw::QuotedTailOfFreshPair, 1usize),
    ] {
        for &first in EXACT_DATA {
            for &second in EXACT_DATA {
                let source = format!(
                    "10 {selector} 10 111 10 001 {first} 01 10 001 {second} 01 01 01"
                );
                let original_bytes = physical(&source);
                let before = execute(&original_bytes);
                let original_ir = lower_canonical_t5_bytes(&original_bytes)
                    .expect("only domain-law admitted executable source may be optimized");
                let optimized = optimize_canonical_t5(&original_bytes)
                    .expect("proof-checked optimizer must admit exact D3 selector+pair");
                assert_eq!(optimized.canonical_t5_sha256,
                    sens::sha256_source(&original_bytes));
                assert_eq!(optimized.original_nodes, 4);
                assert_eq!(optimized.optimized_nodes, 1);
                assert_eq!(optimized.witnesses.len(), 1);
                assert_eq!(optimized.witnesses[0].law, rule);
                assert!(replay_optimizer_witness(&original_ir, &optimized).unwrap());

                let selected = if chosen_index == 0 { first } else { second };
                let independently_encoded_quote = physical(&format!("10 001 {selected} 01"));
                let after = execute(&independently_encoded_quote);
                assert_eq!(before.value, after.value,
                    "SENS value changed: selector={selector}, left={first}, right={second}");
                assert_eq!(before.output, after.output,
                    "SENS output changed: selector={selector}, left={first}, right={second}");
                let expected_ir = lower_canonical_t5_bytes(&independently_encoded_quote)
                    .expect("independent quote must lower with SENS compiler law");
                assert_eq!(optimized.ir, expected_ir.ir,
                    "rewrite must produce EXACT same SENS-law verified IR as independent quote");
                // The source carrier is immutable and remains canonical physical T5.
                assert_eq!(physical(&source), original_bytes);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 32);
    println!("CML_SENS_OPTIMIZER_EXHAUSTIVE_QUOTE_ORACLE_PASS cases={checked} rewrite_laws=2 datum_shapes=4");
}

#[test]
fn non_quote_pair_operand_never_allows_selector_elimination() {
    // Even when a bare D1 literal happens to be a constant, replacing
    // a pair argument with it is NOT among our independently proven rules.
    // Do not generalize rewrite to arbitrary expressions or eager effects.
    for selector in ["100", "011"] {
        let canonical = physical(&format!(
            "10 {selector} 10 111 10 001 0 01 10 01 01 01"
        ));
        let original_ir = lower_canonical_t5_bytes(&canonical)
            .expect("bare D2 empty-list literal is admitted as an IR operand");
        let optimized = optimize_canonical_t5(&canonical).unwrap();
        assert!(optimized.witnesses.is_empty());
        assert_eq!(optimized.ir, original_ir.ir);
        assert_eq!(optimized.original_nodes, optimized.optimized_nodes);
    }
    println!("CML_SENS_OPTIMIZER_NO_UNPROVED_EAGER_REWRITE_PASS");
}
