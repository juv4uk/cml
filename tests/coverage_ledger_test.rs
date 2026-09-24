use std::collections::HashSet;

use cml::canon::{CANON_BUILD_SOURCE_SEMANTIC_IDS, CANON_OPERATIONS_TABLE};
use cml::coverage::{AdmissionState, CoverageLedger};

#[test]
fn build_source_ledger_covers_every_upstream_identity_once() {
    let ledger = CoverageLedger::build_source();

    assert_eq!(ledger.upstream_channel, "build-source");
    assert_eq!(ledger.rows.len(), CANON_BUILD_SOURCE_SEMANTIC_IDS.len());
    assert!(
        ledger.rows.len() > CANON_OPERATIONS_TABLE.len(),
        "the upstream denominator must remain larger than CML's admitted operation slice"
    );

    let ids: HashSet<_> = ledger.rows.iter().map(|row| row.semantic_id).collect();
    assert_eq!(ids.len(), ledger.rows.len(), "semantic IDs must be unique");
    assert!(
        ids.iter().all(|id| {
            let spelling = id.to_string();
            spelling.len() == 8 && spelling.chars().all(|c| c == '0' || c == '1')
        }),
        "build-source semantic identities remain opaque 8-bit binary spellings"
    );
}

#[test]
fn every_cml_operation_is_joined_to_the_upstream_denominator_with_existing_evidence() {
    let ledger = CoverageLedger::build_source();

    for operation in CANON_OPERATIONS_TABLE {
        let row = ledger
            .row(operation.semantic_id)
            .expect("every admitted CML operation must exist in the build-source denominator");

        assert_eq!(row.admission, AdmissionState::SourceAdmitted);
        assert_eq!(row.operation_status, Some(operation.status));
        assert_eq!(row.evidence, Some(operation.provenance_witness));
    }
}

#[test]
fn upstream_known_does_not_collapse_into_supported() {
    let ledger = CoverageLedger::build_source();

    assert!(
        ledger
            .rows
            .iter()
            .any(|row| row.admission == AdmissionState::NotYetAdmitted),
        "the ledger must preserve upstream-known but not-yet-admitted identities"
    );
}

#[test]
fn summary_partitions_the_build_source_denominator_and_keeps_backend_evidence_bounded() {
    let ledger = CoverageLedger::build_source();
    let summary = ledger.summary();

    assert_eq!(summary.semantic_identities, ledger.rows.len());
    assert_eq!(
        summary.source_admitted + summary.not_yet_admitted,
        summary.semantic_identities
    );
    assert_eq!(summary.x86_executable, 4);
    assert_eq!(summary.x86_assembly_witness_only, 3);

    assert!(
        ledger
            .rows
            .iter()
            .filter(|row| row.admission == AdmissionState::NotYetAdmitted)
            .all(|row| row.backend_evidence.is_empty()),
        "not-yet-admitted identities must never receive backend evidence"
    );
}

#[test]
fn ledger_records_a_nonzero_digest_of_the_exact_build_source_registry_input() {
    let ledger = CoverageLedger::build_source();
    assert_ne!(ledger.registry_digest_fnv1a64, 0);
}
