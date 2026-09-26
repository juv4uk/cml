use sens::Sid8;
use std::fs;

use cml::coverage::{BackendEvidenceState, CoverageLedger};

const LIMITED: &[(sens::Sid8, &str)] = &[
    (
        sens::sid!(00001100),
        "named_definition_uses_a_lexical_let_binding",
    ),
    (
        sens::sid!(00001101),
        "out_of_line_named_self_tail_recursion_reuses_its_native_frame",
    ),
];

fn read(path: &str) -> String {
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
fn x86_add_and_sub_are_executable_but_fixnum_representation_limited() {
    let ledger = CoverageLedger::build_source();
    let matrix = read("capability-matrix.lisp");
    let x86 = x86_section(&matrix);
    let witnesses = read("tests/x86_freestanding_test.rs");

    assert!(
        x86.contains("(integer . supported)"),
        "x86 integer mechanism must remain explicitly supported"
    );
    assert!(
        x86.contains("(rational . unsupported)"),
        "x86 exact rational representation must remain explicitly unsupported"
    );

    for (semantic_id, runtime_witness) in LIMITED {
        let row = ledger
            .row(*semantic_id)
            .unwrap_or_else(|| panic!("coverage row {semantic_id} must exist"));
        let evidence = row
            .backend_evidence
            .iter()
            .find(|e| e.backend == "x86-freestanding")
            .unwrap_or_else(|| panic!("{semantic_id} must have x86 evidence"));

        assert_eq!(
            evidence.state,
            BackendEvidenceState::ExecutableRepresentationLimited
        );
        assert!(
            evidence.evidence.contains(runtime_witness),
            "{semantic_id} must name its runtime witness"
        );
        let operation_key = if *semantic_id == sens::sid!(00001100) {
            "add"
        } else {
            "sub"
        };
        assert!(
            x86.contains(&format!("({operation_key} . {:?})", evidence.evidence)),
            "x86 matrix evidence must exactly match ledger evidence for {semantic_id}"
        );
        assert!(
            witnesses.contains(&format!("fn {runtime_witness}(")),
            "runtime witness {runtime_witness} must exist in pushed test source"
        );

        let limit = evidence
            .representation_limit
            .expect("representation-limited evidence must name its limit");
        assert!(limit.contains("fixnum"));
        assert!(limit.contains("#105"));
        assert!(limit.contains("#137"));
    }

    assert!(
        x86.contains("(add . \"x86_freestanding_test.rs:"),
        "x86 matrix must carry concrete add evidence"
    );
    assert!(
        x86.contains("(sub . \"x86_freestanding_test.rs:"),
        "x86 matrix must carry concrete sub evidence"
    );
}

#[test]
fn summary_keeps_representation_limited_out_of_full_executable_count() {
    let summary = CoverageLedger::build_source().summary();

    assert_eq!(summary.x86_executable, 4);
    assert_eq!(summary.x86_assembly_witness_only, 3);
    assert_eq!(summary.x86_representation_limited, 2);
}
