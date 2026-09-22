use std::collections::BTreeSet;

use cml::canon::{CANON_OPERATIONS_TABLE, CANON_SUPPORTED_PIN_SEMANTIC_IDS};
use cml::coverage::{AdmissionState, CoverageLedger};

#[test]
fn supported_pin_ledger_covers_every_upstream_identity_once() {
    let ledger = CoverageLedger::supported_pin();

    assert_eq!(ledger.upstream_channel, "supported-pin");
    assert_eq!(ledger.rows.len(), CANON_SUPPORTED_PIN_SEMANTIC_IDS.len());
    assert!(
        ledger.rows.len() > CANON_OPERATIONS_TABLE.len(),
        "the upstream denominator must remain larger than CML's admitted operation slice"
    );

    let ids: BTreeSet<_> = ledger.rows.iter().map(|row| row.semantic_id).collect();
    assert_eq!(ids.len(), ledger.rows.len(), "semantic IDs must be unique");
    assert!(
        ids.iter()
            .all(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())),
        "supported-pin semantic identities remain opaque numeric IDs"
    );
}

#[test]
fn every_cml_operation_is_joined_to_the_upstream_denominator_with_existing_evidence() {
    let ledger = CoverageLedger::supported_pin();

    for operation in CANON_OPERATIONS_TABLE {
        let row = ledger
            .row(operation.semantic_id)
            .expect("every admitted CML operation must exist in the supported-pin denominator");

        assert_eq!(row.admission, AdmissionState::SourceAdmitted);
        assert_eq!(row.operation_status, Some(operation.status));
        assert_eq!(row.evidence, Some(operation.provenance_witness));
    }
}

#[test]
fn upstream_known_does_not_collapse_into_supported() {
    let ledger = CoverageLedger::supported_pin();

    assert!(
        ledger
            .rows
            .iter()
            .any(|row| row.admission == AdmissionState::NotYetAdmitted),
        "the ledger must preserve upstream-known but not-yet-admitted identities"
    );
}

#[test]
fn summary_partitions_the_supported_pin_denominator_without_backend_claims() {
    let ledger = CoverageLedger::supported_pin();
    let summary = ledger.summary();

    assert_eq!(summary.semantic_identities, ledger.rows.len());
    assert_eq!(
        summary.source_admitted + summary.not_yet_admitted,
        summary.semantic_identities
    );

    assert!(
        ledger.rows.iter().all(|row| row.backend_evidence.is_none()),
        "backend execution evidence must be populated only by later evidence-backed slices"
    );
}

#[test]
fn ledger_records_a_nonzero_digest_of_the_exact_supported_pin_registry_input() {
    let ledger = CoverageLedger::supported_pin();
    assert_ne!(ledger.registry_digest_fnv1a64, 0);
}
