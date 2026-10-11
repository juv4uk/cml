//! Historical x86 add/sub remain target-fixnum machine witnesses.
//! They are NOT automatically identities or proofs for current D3..D9 roles.
use std::fs;
use cml::coverage::CoverageLedger;

const HISTORICAL_LIMITS: &[(&str, &str)] = &[
    ("add", "named_definition_uses_a_lexical_let_binding"),
    ("sub", "out_of_line_named_self_tail_recursion_reuses_its_native_frame"),
];

fn read(path: &str) -> String {
    let source = std::path::Path::new(path);
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let resolved = if source.is_file() {
        source.to_path_buf()
    } else {
        base.join("../../").join(path)
    };
    fs::read_to_string(&resolved)
        .unwrap_or_else(|e| panic!("must read {}: {e}", resolved.display()))
}

fn x86_section(matrix: &str) -> &str {
    let start = matrix.find("(x86-freestanding")
        .expect("capability matrix needs x86-freestanding");
    let tail = &matrix[start..];
    let end = tail.find("\n\n\n ; Global").or_else(|| tail.find("\n\n ; Global"))
        .expect("x86 section must end before global admission");
    &tail[..end]
}

#[test]
fn historical_fixnum_machine_witness_is_preserved_and_representation_limited() {
    let matrix = read("capability-matrix.lisp");
    let x86 = x86_section(&matrix);
    let witnesses = read("tests/x86_freestanding_test.rs");
    assert!(x86.contains("(integer . supported)"));
    assert!(x86.contains("(rational . unsupported)"));
    for &(operation, witness) in HISTORICAL_LIMITS {
        assert!(x86.contains(&format!("({operation} . \"x86_freestanding_test.rs:")));
        assert!(x86.contains(witness));
        assert!(witnesses.contains(&format!("fn {witness}(")));
    }
    // Historical target fixnums do not attest SENS exact number or current
    // domain semantics. Never copy their result to the new coverage ledger.
    let ledger = CoverageLedger::try_current_sens().unwrap();
    assert!(ledger.rows.iter().all(|row| row.backend_evidence.is_empty()));
}

#[test]
fn no_historical_fixnum_proof_is_counted_as_current_full_native_execution() {
    let summary = CoverageLedger::try_current_sens().unwrap().summary();
    assert_eq!(summary.x86_executable, 0);
    assert_eq!(summary.x86_assembly_witness_only, 0);
    assert_eq!(summary.x86_representation_limited, 0);
}
