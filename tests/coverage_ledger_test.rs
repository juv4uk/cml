use std::collections::BTreeSet;

use cml::canon::{CANON_OPERATIONS_TABLE, CANON_UPSTREAM_SEMANTIC_IDS};
use cml::coverage::{AdmissionState, CoverageLedger};

#[test]
fn pinned_submodule_ledger_covers_every_upstream_identity_once() {
    let ledger = CoverageLedger::pinned_submodule();
    assert_eq!(ledger.upstream_channel, "pinned-submodule");
    assert_eq!(ledger.rows.len(), CANON_UPSTREAM_SEMANTIC_IDS.len());

    let ids: BTreeSet<_> = ledger.rows.iter().map(|row| row.semantic_id).collect();
    assert_eq!(ids.len(), ledger.rows.len());
    assert!(
        ids.iter()
            .all(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())),
        "every coverage row must preserve an opaque numeric-only semantic ID"
    );
}

#[test]
fn every_admitted_operation_is_visible_with_evidence() {
    let ledger = CoverageLedger::pinned_submodule();

    for op in CANON_OPERATIONS_TABLE {
        let row = ledger
            .row(op.semantic_id)
            .expect("every admitted operation must be present in the upstream denominator");
        assert_eq!(row.admission, AdmissionState::SourceAdmitted);
        assert_eq!(row.operation_status, Some(op.status));
        assert_eq!(row.evidence, Some(op.provenance_witness));
    }
}

#[test]
fn unadmitted_upstream_identity_is_not_misreported_as_supported() {
    let ledger = CoverageLedger::pinned_submodule();

    assert!(
        ledger
            .rows
            .iter()
            .any(|row| row.admission == AdmissionState::NotYetAdmitted),
        "the upstream registry must remain larger than CML's admitted operation slice"
    );
}

#[test]
fn summary_partitions_the_denominator() {
    let ledger = CoverageLedger::pinned_submodule();
    let summary = ledger.summary();

    assert_eq!(summary.semantic_identities, ledger.rows.len());
    assert_eq!(
        summary.source_admitted + summary.not_yet_admitted,
        summary.semantic_identities
    );
}
