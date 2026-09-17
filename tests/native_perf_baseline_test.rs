//! Conformance and determinism tests for the Native Performance Baseline (#53).
//!
//! Verifies:
//! 1. Corpus integrity: active workloads record mechanism observations without owning Lisp truth.
//! 2. Structural metrics determinism: byte count, instruction count, branches, and memory ops are stable.
//! 3. Lane outcomes remain observable independently from semantic verdict authority.
//! 4. Fail-closed policy: unadmitted dynamic operations fail closed.
//! 5. Machine-readable report serialization (JSON & S-expression).

use cml::native_baseline::generate_baseline_report;

#[test]
fn test_corpus_records_mechanism_outcomes_without_semantic_answer_keys() {
    let report = generate_baseline_report("head", "test-mylisp-pin");

    // Workload 1: scalar-add remains an admitted mechanism workload.
    let w1 = report
        .workloads
        .iter()
        .find(|w| w.id == "scalar-add")
        .expect("scalar-add workload must be present");
    assert!(w1.admitted);
    for lane in &w1.lanes {
        assert_eq!(lane.status, "OK");
        assert_eq!(lane.outcome, "42");
        assert!(lane.runtime.is_some());
    }

    // Workload 2: counted-loop-sum-1000 remains an admitted mechanism workload.
    let w2 = report
        .workloads
        .iter()
        .find(|w| w.id == "counted-loop-sum-1000")
        .expect("counted-loop workload must be present");
    assert!(w2.admitted);
    for lane in &w2.lanes {
        assert_eq!(lane.status, "OK");
        assert_eq!(lane.outcome, "500500");
        assert!(lane.runtime.is_some());
    }

    // Workload 3: buffer-map-i32 remains an admitted mechanism workload.
    let w3 = report
        .workloads
        .iter()
        .find(|w| w.id == "buffer-map-i32")
        .expect("buffer-map-i32 workload must be present");
    assert!(w3.admitted);
    for lane in &w3.lanes {
        assert_eq!(lane.status, "OK");
        assert_eq!(lane.outcome, "#i32(2 3 4 5 6)");
    }

    // Workload 4 is a compiler admission-policy observation, not a Lisp answer key.
    let w4 = report
        .workloads
        .iter()
        .find(|w| w.id == "unsupported-dynamic-fail-closed")
        .expect("unsupported dynamic workload must be present");
    assert!(!w4.admitted, "unadmitted workload must have admitted=false");
    assert_eq!(w4.lanes[0].status, "REJECTED");
}

#[test]
fn test_structural_metrics_stability_and_determinism() {
    let report1 = generate_baseline_report("head-1", "test-mylisp-pin");
    let report2 = generate_baseline_report("head-2", "test-mylisp-pin");

    let w1_1 = report1
        .workloads
        .iter()
        .find(|w| w.id == "scalar-add")
        .unwrap();
    let w1_2 = report2
        .workloads
        .iter()
        .find(|w| w.id == "scalar-add")
        .unwrap();
    assert_eq!(w1_1.structural, w1_2.structural);

    let st1 = w1_1.structural.as_ref().unwrap();
    assert_eq!(
        st1.code_bytes, 24,
        "scalar-add must be exactly 24 bytes in unoptimized baseline"
    );
    assert_eq!(st1.instruction_count, 4);
    assert_eq!(st1.load_count, 0);
    assert_eq!(st1.store_count, 0);
    assert_eq!(st1.branch_count, 0);
    assert_eq!(st1.call_count, 0);

    let w2_1 = report1
        .workloads
        .iter()
        .find(|w| w.id == "counted-loop-sum-1000")
        .unwrap();
    let w2_2 = report2
        .workloads
        .iter()
        .find(|w| w.id == "counted-loop-sum-1000")
        .unwrap();
    assert_eq!(w2_1.structural, w2_2.structural);

    let st2 = w2_1.structural.as_ref().unwrap();
    assert_eq!(
        st2.code_bytes, 34,
        "counted-loop must be exactly 34 bytes in unoptimized baseline"
    );
    assert_eq!(st2.instruction_count, 6); // mov, mov, add, sub, jnz, ret
    assert_eq!(st2.branch_count, 1);
    assert_eq!(st2.call_count, 0);
}

#[test]
fn test_report_serialization_formats() {
    let report = generate_baseline_report("abc1234", "test-mylisp-pin");
    let json = report.to_json();
    assert!(json.starts_with('{'));
    assert!(json.ends_with("}\n"));
    assert!(json.contains("\"target_cpu_profile\""));
    assert!(json.contains("\"cml_commit\": \"abc1234\""));
    assert!(json.contains("\"workloads\""));

    let sexpr = report.to_sexpr();
    assert!(sexpr.starts_with("((kind . cml-native-perf-baseline)"));
    assert!(sexpr.contains("(cml-commit . \"abc1234\")"));
    assert!(sexpr.contains("(workloads ."));
}

#[test]
fn active_baseline_report_does_not_embed_semantic_answer_keys() {
    let report = generate_baseline_report("authority-red", "test-mylisp-pin");
    let json = report.to_json();

    assert!(
        !json.contains("\"expected_outcome\""),
        "active CML benchmark reports must not embed a CML-owned expected Lisp answer"
    );
    assert!(
        !json.contains("\"correctness_verdict\""),
        "CML must not mint PASS/FAIL semantic verdicts from local constants"
    );
    assert!(
        json.contains("\"verdict_source\""),
        "each active row must identify where semantic judgment belongs"
    );
    assert!(
        json.contains("\"upstream_witness_ref\""),
        "each active Lisp workload must carry pinned upstream witness provenance"
    );

    let counted = report
        .workloads
        .iter()
        .find(|w| w.id == "counted-loop-sum-1000")
        .expect("counted-loop workload must be present");
    assert!(
        counted.lisp_source.contains("identity-relation"),
        "current counted-loop source must consume the explicit eq result domain"
    );
    assert!(
        !counted.lisp_source.contains("(t (sum"),
        "historical truthy two-part cond source must not remain active"
    );
}
