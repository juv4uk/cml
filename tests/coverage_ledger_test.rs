//! Current coverage counts only SENS-executable-law verified, exact-width
//! D3/D4 compiler roles. Historical byte-indexed source and backend tables
//! are not interchangeable with the ratified domain ladder.
use std::collections::HashSet;

use cml::coverage::{AdmissionState, CoverageLedger};

#[test]
fn source_owned_compiler_corpus_has_nine_unique_exact_domain_identities() {
    let ledger = CoverageLedger::try_current_sens().unwrap();
    assert_eq!(ledger.upstream_channel, "current-sens-executable-compiler-law");
    assert_eq!(ledger.rows.len(), 9);
    let identities: HashSet<_> = ledger.rows.iter().map(|row| {
        let width = row.identity.width();
        let bits = row.identity.packed_bits();
        format!("D{width}:{bits:0width$b}", width = width as usize)
    }).collect();
    assert_eq!(identities.len(), 9);
    assert_eq!(identities.iter().filter(|id| id.starts_with("D3:")).count(), 7);
    assert_eq!(identities.iter().filter(|id| id.starts_with("D4:")).count(), 2);
}

#[test]
fn every_compiler_role_is_rechecked_by_executable_sens_law() {
    let ledger = CoverageLedger::try_current_sens().unwrap();
    for row in &ledger.rows {
        let law = sens::compiler_semantic_input_from_sens(row.identity)
            .unwrap().expect("no unproved compiler role");
        assert_eq!(row.execution_role, law.lowering_role);
        assert_eq!(row.evidence, law.proof_ref);
        assert_eq!(row.authority_sha256, law.authority_sha256);
        assert_eq!(row.admission, AdmissionState::SourceAdmitted);
        assert_eq!(ledger.row(row.identity), Some(row));
    }
}

#[test]
fn current_coverage_denominator_does_not_implicitly_include_all_domain_words() {
    let summary = CoverageLedger::try_current_sens().unwrap().summary();
    assert_eq!(summary.compiler_role_identities, 9);
    assert_eq!(summary.source_admitted, 9);
    assert_eq!(summary.not_yet_admitted, 0);
}

#[test]
fn historical_x86_evidence_is_never_promoted_to_exact_compiler_roles() {
    let ledger = CoverageLedger::try_current_sens().unwrap();
    assert!(ledger.rows.iter().all(|row| row.backend_evidence.is_empty()));
    let summary = ledger.summary();
    assert_eq!(summary.x86_executable, 0);
    assert_eq!(summary.x86_assembly_witness_only, 0);
    assert_eq!(summary.x86_representation_limited, 0);
}

#[test]
fn the_executable_law_authority_sha_is_not_the_legacy_fnv_registry_digest() {
    let ledger = CoverageLedger::try_current_sens().unwrap();
    assert_eq!(ledger.authority_sha256.len(), 64);
    assert!(ledger.authority_sha256.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(ledger.authority_sha256,
        sens::sha256_source(include_bytes!("../external/sens/language-contract.lisp"))
            .iter().map(|b| format!("{b:02x}")).collect::<String>());
}
