//! Historical native x86 witnesses remain mechanism evidence only.
//! A matching English name or retired byte address cannot authorize a new
//! ratified D3/D4 semantic identity; that proof comes from SENS's law.
use cml::coverage::CoverageLedger;
use std::fs;

const HISTORICAL_X86_WITNESSES: &[(&str, &str)] = &[
    (
        "quote",
        "compiler_corpus_quote_radio_returns_the_interned_symbol",
    ),
    ("atom", "primitive_slice_uses_only_ratified_runtime_imports"),
    (
        "eq",
        "compiler_corpus_eq_on_matching_and_differing_quoted_symbols",
    ),
    ("cons", "frozen_cons_fixture_is_deterministic_and_assembles"),
    ("car", "primitive_slice_uses_only_ratified_runtime_imports"),
    ("cdr", "primitive_slice_uses_only_ratified_runtime_imports"),
    (
        "cond",
        "standalone_cond_true_false_branch_selection_witness",
    ),
];

fn read_repo(path: &str) -> String {
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
    let start = matrix
        .find("(x86-freestanding")
        .expect("capability matrix needs x86-freestanding");
    let tail = &matrix[start..];
    let end = tail
        .find("\n\n\n ; Global")
        .or_else(|| tail.find("\n\n ; Global"))
        .expect("x86 section must end before global admission");
    &tail[..end]
}

#[test]
fn historical_native_witnesses_still_exist_but_are_not_current_sens_evidence() {
    let matrix = read_repo("capability-matrix.lisp");
    let x86 = x86_section(&matrix);
    let test_source = read_repo("tests/x86_freestanding_test.rs");
    for &(legacy_surface, witness) in HISTORICAL_X86_WITNESSES {
        assert!(x86.contains(&format!("({legacy_surface} . supported)")));
        assert!(
            x86.contains(witness),
            "the historical matrix lost {legacy_surface} evidence"
        );
        assert!(
            test_source.contains(&format!("fn {witness}(")),
            "historical test witness disappeared: {witness}"
        );
    }
    let current = CoverageLedger::try_current_sens().unwrap();
    assert_eq!(current.rows.len(), 9);
    assert!(
        current
            .rows
            .iter()
            .all(|row| row.backend_evidence.is_empty()),
        "legacy native witness cannot grant current domain native execution"
    );
}

#[test]
fn current_exact_domain_summary_never_infers_native_execution_from_historical_name() {
    let summary = CoverageLedger::try_current_sens().unwrap().summary();
    assert_eq!(summary.x86_executable, 0);
    assert_eq!(summary.x86_assembly_witness_only, 0);
    assert_eq!(summary.x86_representation_limited, 0);
}
