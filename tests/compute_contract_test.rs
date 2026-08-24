use std::fs;

#[test]
fn machine_readable_compute_contract_matches_analysis_m0() {
    let contract = fs::read_to_string("compute-contract.my").unwrap();
    for required in [
        "(version . (0 7))",
        "(status . analysis-only)",
        "(unknown-facts . reject)",
        "(fallback . cpu)",
        "(implicit-exact-to-inexact-conversion . forbidden)",
        "(typed-buffer-ir . present-from-my-lisp-contract-2.2)",
        "(kernel-ir . (parameter exact-integer checked-add))",
        "(required-i32-overflow-proof . true)",
        "(f32-rounding-contract . affine-x-plus-integer-per-literal-bit-equivalence)",
        "(cpu-compute-backend . (present-reference i32-map-range-proven f32-map-affine-proven))",
        "(differential-oracle . \"my-lisp path dev-dependency\")",
        "(gpu-emitter . absent)",
    ] {
        assert!(contract.contains(required), "compute contract lost {required}");
    }
}
