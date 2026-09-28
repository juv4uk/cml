use sens::Sens8;
use std::fs;

use cml::canon::find_operation_by_id;
use cml::coverage::{BackendEvidenceState, CoverageLedger};

const EXPECTED: &[(sens::Sens8, BackendEvidenceState)] = &[
    (sens::sid!(00000001), BackendEvidenceState::Executable),
    (sens::sid!(00000010), BackendEvidenceState::AssemblyWitness),
    (sens::sid!(00000011), BackendEvidenceState::Executable),
    (sens::sid!(00000100), BackendEvidenceState::Executable),
    (sens::sid!(00000101), BackendEvidenceState::AssemblyWitness),
    (sens::sid!(00000110), BackendEvidenceState::AssemblyWitness),
    (sens::sid!(00000111), BackendEvidenceState::Executable),
];

fn read_repo(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("must read {path}: {error}"))
}

fn x86_section(matrix: &str) -> &str {
    let start = matrix
        .find("(x86-freestanding")
        .expect("capability matrix must contain x86-freestanding");
    let tail = &matrix[start..];
    let end = tail
        .find("\n\n\n ; Global")
        .or_else(|| tail.find("\n\n ; Global"))
        .expect("x86 section must end before global admission");
    &tail[..end]
}

#[test]
fn x86_canon_evidence_reconciles_matrix_ledger_and_pushed_witnesses() {
    let matrix = read_repo("capability-matrix.lisp");
    let x86 = x86_section(&matrix);
    let witness_source = read_repo("tests/x86_freestanding_test.rs");
    let ledger = CoverageLedger::build_source();

    let mut x86_rows = 0usize;
    for (semantic_id, expected_state) in EXPECTED {
        let operation = find_operation_by_id(*semantic_id)
            .unwrap_or_else(|| panic!("Canon operation {semantic_id} must exist"));
        assert!(
            x86.contains(&format!("({} . supported)", operation.canonical_name)),
            "x86 capability matrix must keep {} supported while ledger claims x86 evidence",
            operation.canonical_name
        );

        let row = ledger
            .row(*semantic_id)
            .unwrap_or_else(|| panic!("ledger row {semantic_id} must exist"));
        let evidence: Vec<_> = row
            .backend_evidence
            .iter()
            .filter(|e| e.backend == "x86-freestanding")
            .collect();
        assert_eq!(
            evidence.len(),
            1,
            "{semantic_id} must have exactly one bounded x86 evidence classification"
        );
        let evidence = evidence[0];
        assert_eq!(
            evidence.state, *expected_state,
            "{semantic_id} x86 evidence class must not be upgraded implicitly"
        );
        assert!(
            x86.contains(&format!(
                "({} . {:?})",
                operation.canonical_name, evidence.evidence
            )),
            "capability matrix must own the same named evidence as the ledger for {}",
            operation.canonical_name
        );

        let (_, tests) = evidence
            .evidence
            .split_once(": ")
            .expect("evidence must be path: test-name(s)");
        for test_name in tests.split("; ") {
            assert!(
                witness_source.contains(&format!("fn {test_name}(")),
                "named x86 witness {test_name} must exist in pushed test source"
            );
        }
        x86_rows += 1;
    }

    let actual_x86_rows = ledger
        .rows
        .iter()
        .filter(|row| {
            row.backend_evidence
                .iter()
                .any(|e| e.backend == "x86-freestanding")
        })
        .count();
    assert_eq!(x86_rows, EXPECTED.len());
    assert!(
        actual_x86_rows >= x86_rows,
        "later evidence-backed x86 slices may add rows, but must not remove the seven Canon-core rows"
    );
}

#[test]
fn x86_summary_distinguishes_executable_from_assembly_only() {
    let summary = CoverageLedger::build_source().summary();

    assert_eq!(summary.x86_executable, 4);
    assert_eq!(summary.x86_assembly_witness_only, 3);
}
