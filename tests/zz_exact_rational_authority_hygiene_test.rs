use std::fs;
use std::path::PathBuf;

#[test]
fn c_backend_exact_rational_semantics_do_not_have_local_answer_oracles() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/c_backend_test.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));

    let stale_local_oracles = [
        "fn c_backend_lowers_rational_add_matches_oracle",
        "fn c_backend_lowers_rational_sub_matches_oracle",
        "fn c_backend_lowers_rational_mul_matches_oracle",
        "fn c_backend_lowers_rational_div_matches_oracle",
        "fn c_backend_lowers_rational_reduces_matches_oracle",
        "fn c_backend_lowers_rational_int_mix_matches_oracle",
        "fn c_backend_lowers_rational_unary_minus_matches_oracle",
    ];

    let found: Vec<_> = stale_local_oracles
        .into_iter()
        .filter(|needle| source.contains(needle))
        .collect();

    assert!(
        found.is_empty(),
        "#123 requires semantic rational answers to come from pinned Lisp authority, not tests/c_backend_test.rs; stale local oracles: {found:?}"
    );
}
