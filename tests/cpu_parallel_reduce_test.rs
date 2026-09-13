use cml::compute::{
    AdmissionBlocker, ComputeBackend, ComputeExecutionError, CpuComputeBackend, GroupingLaw,
    NumericDomain, ParallelCpuComputeBackend, analyze,
};
use cml::ir::{BufferLiteral, Ir};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

#[test]
fn reduction_proof_verifies_associative_addition_over_bounded_i32() {
    let source = "(reduce + 0 #i32(10 20 30))";
    let ir = lower_one(source);
    let analysis = analyze(&ir);

    let proof = analysis
        .reduction_proof
        .expect("reduction proof must be present for reduce expression");

    assert_eq!(
        proof.grouping_law,
        GroupingLaw::Associative { identity: Some(0) }
    );
    assert!(proof.overflow_invariant);
    assert!(proof.contiguous_storage);
    assert!(proof.pure_kernel);
    assert_eq!(proof.numeric_domain, NumericDomain::FixedWidthInteger);
    assert!(proof.is_parallel_eligible());
}

#[test]
fn reduction_proof_rejects_non_associative_kernel_and_falls_back_to_sequential() {
    // Operation: (lambda (acc x) (+ (+ acc acc) x)), i.e. 2*acc + x.
    // Non-associative: (1 op 1) op 1 = 7, but 1 op (1 op 1) = 5.
    let source = "(reduce (lambda (acc x) (+ (+ acc acc) x)) 0 #i32(1 2 3))";
    let ir = lower_one(source);
    let analysis = analyze(&ir);

    let proof = analysis
        .reduction_proof
        .expect("reduction proof must be present");
    assert_eq!(proof.grouping_law, GroupingLaw::NonAssociative);
    assert!(!proof.is_parallel_eligible());

    // Proves that when proof is not parallel-eligible, it falls back to sequential reference execution
    let report = ParallelCpuComputeBackend::new(4)
        .execute_diagnostic(&ir)
        .unwrap();
    assert_eq!(
        report.workers_used, 1,
        "must not parallelize non-associative operation"
    );
    assert_eq!(report.unique_threads, 1);
    // Sequential reference: acc=0 -> (+ 0 1)=1 -> (+ 2 2)=4 -> (+ 8 3)=11
    assert_eq!(report.output, BufferLiteral::I32(vec![11]));

    let reference = CpuComputeBackend.execute(&ir).unwrap();
    assert_eq!(report.output, reference);
}

#[test]
fn reduction_proof_rejects_potential_overflow_regrouping_sensitivity() {
    // Array where sequential fold does not overflow:
    // 0 + 1.5B = 1.5B
    // 1.5B - 1.0B = 0.5B
    // 0.5B + 1.0B = 1.5B
    // Total sum = 1.5B fits in i32.
    // BUT sum of absolute values = 1.5B + 1.0B + 1.0B = 3.5B > i32::MAX.
    // Regrouping (e.g. tree reduction grouping element 0 and 2: 1.5B + 1.0B = 2.5B) would overflow!
    let source = "(reduce + 0 #i32(1500000000 -1000000000 1000000000))";
    let ir = lower_one(source);
    let analysis = analyze(&ir);

    let proof = analysis
        .reduction_proof
        .expect("reduction proof must be present");
    assert!(
        !proof.overflow_invariant,
        "must detect that tree regrouping could exceed i32 range"
    );
    assert!(!proof.is_parallel_eligible());

    // Verified safe fallback to sequential execution without regrouping:
    let report = ParallelCpuComputeBackend::new(4)
        .execute_diagnostic(&ir)
        .unwrap();
    assert_eq!(report.workers_used, 1);
    assert_eq!(report.unique_threads, 1);
    assert_eq!(report.output, BufferLiteral::I32(vec![1500000000]));

    let reference = CpuComputeBackend.execute(&ir).unwrap();
    assert_eq!(report.output, reference);
}

#[test]
fn actual_sequential_overflow_fails_closed_in_both_backends() {
    let source = format!("(reduce + 0 #i32({} 10))", i32::MAX);
    let ir = lower_one(&source);

    let ref_err = CpuComputeBackend.execute(&ir).unwrap_err();
    let par_err = ParallelCpuComputeBackend::new(4).execute(&ir).unwrap_err();

    assert!(matches!(
        ref_err,
        ComputeExecutionError::NotEligible(ref blockers)
            if blockers.contains(&AdmissionBlocker::IntegerOverflowNotProven)
    ));
    assert_eq!(ref_err, par_err);
}

#[test]
fn parity_across_sequential_reference_and_parallel_workers_1_2_4() {
    let source = "(reduce + 10 #i32(1 2 3 4 5 6 7 8 9 10 11 12 13))";
    let ir = lower_one(source);

    let seq = CpuComputeBackend.execute(&ir).unwrap();
    let p1 = ParallelCpuComputeBackend::new(1).execute(&ir).unwrap();
    let p2 = ParallelCpuComputeBackend::new(2).execute(&ir).unwrap();
    let p4 = ParallelCpuComputeBackend::new(4).execute(&ir).unwrap();

    assert_eq!(seq, BufferLiteral::I32(vec![101])); // 10 + 91 = 101
    assert_eq!(p1, seq);
    assert_eq!(p2, seq);
    assert_eq!(p4, seq);
}

#[test]
fn handles_empty_single_and_negative_element_reductions() {
    let empty_ir = lower_one("(reduce + 42 #i32())");
    for w in [1, 2, 4] {
        let res = ParallelCpuComputeBackend::new(w)
            .execute(&empty_ir)
            .unwrap();
        assert_eq!(res, BufferLiteral::I32(vec![42]));
    }

    let single_ir = lower_one("(reduce + 10 #i32(32))");
    for w in [1, 2, 4] {
        let res = ParallelCpuComputeBackend::new(w)
            .execute(&single_ir)
            .unwrap();
        assert_eq!(res, BufferLiteral::I32(vec![42]));
    }

    let neg_ir = lower_one("(reduce + 0 #i32(-10 20 -30 40 -50))");
    for w in [1, 2, 4] {
        let res = ParallelCpuComputeBackend::new(w).execute(&neg_ir).unwrap();
        assert_eq!(res, BufferLiteral::I32(vec![-30]));
    }
}

#[test]
fn proves_parallel_reduction_runs_on_multiple_os_threads_when_eligible() {
    let elements: Vec<i32> = (0..500).map(|x| (x % 10) as i32).collect();
    let source = format!(
        "(reduce + 0 #i32({}))",
        elements
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let ir = lower_one(&source);

    let report_1 = ParallelCpuComputeBackend::new(1)
        .execute_diagnostic(&ir)
        .unwrap();
    assert_eq!(report_1.workers_used, 1);
    assert_eq!(report_1.unique_threads, 1);

    let report_2 = ParallelCpuComputeBackend::new(2)
        .execute_diagnostic(&ir)
        .unwrap();
    assert_eq!(report_2.workers_used, 2);
    assert!(
        report_2.unique_threads >= 2,
        "eligible reduction with workers=2 must execute on at least 2 OS threads (got {})",
        report_2.unique_threads
    );

    let report_4 = ParallelCpuComputeBackend::new(4)
        .execute_diagnostic(&ir)
        .unwrap();
    assert_eq!(report_4.workers_used, 4);
    assert!(
        report_4.unique_threads >= 2,
        "eligible reduction with workers=4 must execute concurrently (got {})",
        report_4.unique_threads
    );

    let reference = CpuComputeBackend.execute(&ir).unwrap();
    assert_eq!(report_1.output, reference);
    assert_eq!(report_2.output, reference);
    assert_eq!(report_4.output, reference);
}

#[test]
fn repeated_parallel_reduction_runs_are_strictly_deterministic() {
    let elements: Vec<i32> = (0..200).map(|x| x - 100).collect();
    let source = format!(
        "(reduce + 50 #i32({}))",
        elements
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let ir = lower_one(&source);

    let backend = ParallelCpuComputeBackend::new(4);
    let first = backend.execute(&ir).unwrap();

    for _ in 0..50 {
        let subsequent = backend.execute(&ir).unwrap();
        assert_eq!(subsequent, first);
    }
}
