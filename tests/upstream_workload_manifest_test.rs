use std::fs;

#[test]
fn upstream_workload_manifest_is_authority_safe_and_current() {
    let manifest = fs::read_to_string("benchmarks/upstream-workloads.lisp")
        .expect("#80 requires benchmarks/upstream-workloads.lisp");

    for forbidden in [
        "(expected .",
        "expected-outcome",
        "expected_outcome",
        "(correctness-verdict .",
        "correctness_verdict",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "upstream workload manifest must not own Lisp semantic answers: found {forbidden}"
        );
    }

    for required in [
        "(id . my-lisp-meta-registry-generator-317)",
        "(id . my-lisp-cli-cold-bootstrap-332)",
        "(id . my-lisp-text-pipeline-333)",
        "(workflow-run . 35261854943)",
        "(compiler-child . \"juv4uk/cml#89\")",
        "(control-blocker . \"juv4uk/cml#90\")",
        "(comparison-blocker . \"juv4uk/cml#92\")",
    ] {
        assert!(
            manifest.contains(required),
            "upstream workload manifest is missing current evidence/provenance: {required}"
        );
    }
}
