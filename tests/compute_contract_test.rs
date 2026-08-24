use std::fs;

#[test]
fn machine_readable_compute_contract_matches_analysis_m0() {
    let contract = fs::read_to_string("compute-contract.my").unwrap();
    for required in [
        "(version . (0 2))",
        "(status . analysis-only)",
        "(unknown-facts . reject)",
        "(fallback . cpu)",
        "(implicit-exact-to-inexact-conversion . forbidden)",
        "(typed-buffer-ir . present-from-my-lisp-contract-2.2)",
        "(cpu-compute-backend . absent)",
        "(gpu-emitter . absent)",
    ] {
        assert!(contract.contains(required), "compute contract lost {required}");
    }
}
