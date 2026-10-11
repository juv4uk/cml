//! Compile ACTUAL root src/coverage.rs with the pinned current SENS, not a
//! substitute implementation. Historical x86 SID witnesses are NOT proof
//! that a ratified D3/D4 identity has any native backend coverage.
#[path = "../../../src/coverage.rs"]
mod root_coverage;

use root_coverage::{AdmissionState, CoverageLedger};

#[test]
fn current_coverage_is_exactly_nine_sens_law_verified_compiler_roles() {
    let ledger = CoverageLedger::try_current_sens().unwrap();
    assert_eq!(ledger.rows.len(), 9);
    assert_eq!(ledger.upstream_channel, "current-sens-executable-compiler-law");
    assert_eq!(ledger.authority_sha256.len(), 64);
    let mut d3 = 0;
    let mut d4 = 0;
    for row in &ledger.rows {
        let record = sens::compiler_semantic_input_from_sens(row.identity)
            .unwrap().expect("the SENS compiler law must admit this coordinate");
        assert_eq!(record.lowering_role, row.execution_role);
        assert_eq!(record.proof_ref, row.evidence);
        assert_eq!(row.authority_sha256, ledger.authority_sha256);
        assert_eq!(row.admission, AdmissionState::SourceAdmitted);
        match row.identity {
            sens::CoreDomainIdentity::D3(_) => d3 += 1,
            sens::CoreDomainIdentity::D4(_) => d4 += 1,
            _ => panic!("unexpected compiler domain in SENS-owned nine-role corpus"),
        }
    }
    assert_eq!((d3, d4), (7, 2));
    let summary = ledger.summary();
    assert_eq!(summary.compiler_role_identities, 9);
    assert_eq!(summary.source_admitted, 9);
    assert_eq!(summary.not_yet_admitted, 0);
    println!("CML_ROOT_CURRENT_COVERAGE_PASS source_roles=9 exact_D3=7 exact_D4=2");
}

#[test]
fn historical_native_sid_evidence_cannot_grant_modern_x86_backend_support() {
    let ledger = CoverageLedger::build_source();
    assert!(ledger.rows.iter().all(|r| r.backend_evidence.is_empty()));
    let summary = ledger.summary();
    assert_eq!(summary.x86_executable, 0);
    assert_eq!(summary.x86_assembly_witness_only, 0);
    assert_eq!(summary.x86_representation_limited, 0);
    println!("CML_ROOT_CURRENT_COVERAGE_NATIVE_UNPROVEN_PASS no_evidence_migration=true");
}

#[test]
fn equal_bits_in_d3_and_d4_never_share_one_coverage_identity() {
    let d3 = sens::CoreDomainIdentity::D3(sens::Bija3::from_word(
        sens::Bit3::new(0b001).unwrap()
    ));
    let d4 = sens::CoreDomainIdentity::D4(sens::CoreD4::from_word(
        sens::Bit4::new(0b0001).unwrap()
    ));
    assert_eq!(d3.packed_bits(), d4.packed_bits());
    assert_ne!(d3.width(), d4.width());
    let ledger = CoverageLedger::build_source();
    let a = ledger.row(d3).unwrap();
    let b = ledger.row(d4);
    // The nine compiler-role corpus does not have to contain D4 0001,
    // but if it does, identity cannot collapse with the D3 entry.
    if let Some(b) = b { assert_ne!(a.identity, b.identity); }
    assert_eq!(a.identity, d3);
}
