use std::time::Instant;

use cml::compute::{
    AdmissionBlocker, ComputeBackend, ComputeExecutionError, CpuComputeBackend,
    ParallelCpuComputeBackend,
};
use cml::execution::{
    BufferId, ExecutionGraph, ExecutionOperation, ExecutionTarget, GraphValue,
    HeterogeneousGraphExecutor, NodeId, ParallelCpuNodeExecutor, PlanNode,
};
use cml::ir::{BufferLiteral, Ir};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

#[test]
fn partition_ranges_balance_across_divisible_and_non_divisible_lengths() {
    assert_eq!(ParallelCpuComputeBackend::partition_ranges(0, 4), vec![]);
    assert_eq!(ParallelCpuComputeBackend::partition_ranges(5, 0), vec![]);
    assert_eq!(
        ParallelCpuComputeBackend::partition_ranges(1, 4),
        vec![(0, 1)]
    );
    assert_eq!(
        ParallelCpuComputeBackend::partition_ranges(6, 2),
        vec![(0, 3), (3, 6)]
    );
    // Non-divisible: 7 elements across 3 workers -> [3, 2, 2]
    assert_eq!(
        ParallelCpuComputeBackend::partition_ranges(7, 3),
        vec![(0, 3), (3, 5), (5, 7)]
    );
    // Non-divisible: 10 elements across 4 workers -> [3, 3, 2, 2]
    assert_eq!(
        ParallelCpuComputeBackend::partition_ranges(10, 4),
        vec![(0, 3), (3, 6), (6, 8), (8, 10)]
    );
}

#[test]
fn parity_across_reference_and_parallel_workers_1_2_4() {
    let source = "(numeric-buffer-map (lambda (x) (+ x 10)) #i32(1 2 3 4 5 6 7 8 9 10 11 12 13))";
    let ir = lower_one(source);

    let reference = CpuComputeBackend.execute(&ir).unwrap();
    let p1 = ParallelCpuComputeBackend::new(1).execute(&ir).unwrap();
    let p2 = ParallelCpuComputeBackend::new(2).execute(&ir).unwrap();
    let p4 = ParallelCpuComputeBackend::new(4).execute(&ir).unwrap();

    assert_eq!(p1, reference);
    assert_eq!(p2, reference);
    assert_eq!(p4, reference);
}

#[test]
fn handles_empty_single_and_negative_element_buffers() {
    let empty_ir = lower_one("(numeric-buffer-map (lambda (x) (+ x 1)) #i32())");
    for w in [1, 2, 4] {
        let res = ParallelCpuComputeBackend::new(w)
            .execute(&empty_ir)
            .unwrap();
        assert_eq!(res, BufferLiteral::I32(vec![]));
    }

    let single_ir = lower_one("(numeric-buffer-map (lambda (x) (+ x 5)) #i32(42))");
    for w in [1, 2, 4] {
        let res = ParallelCpuComputeBackend::new(w)
            .execute(&single_ir)
            .unwrap();
        assert_eq!(res, BufferLiteral::I32(vec![47]));
    }

    let negative_ir = lower_one("(numeric-buffer-map (lambda (x) (+ x -10)) #i32(-5 -15 20))");
    for w in [1, 2, 4] {
        let res = ParallelCpuComputeBackend::new(w)
            .execute(&negative_ir)
            .unwrap();
        assert_eq!(res, BufferLiteral::I32(vec![-15, -25, 10]));
    }
}

#[test]
fn proves_real_work_occurs_on_multiple_os_threads_when_workers_gt_1() {
    let elements: Vec<i32> = (0..500).collect();
    let source = format!(
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32({}))",
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
        "workers=2 must execute on at least 2 distinct OS threads (got {})",
        report_2.unique_threads
    );

    let report_4 = ParallelCpuComputeBackend::new(4)
        .execute_diagnostic(&ir)
        .unwrap();
    assert_eq!(report_4.workers_used, 4);
    assert!(
        report_4.unique_threads >= 2,
        "workers=4 must execute concurrently on OS threads (got {})",
        report_4.unique_threads
    );

    assert_eq!(report_1.output, report_2.output);
    assert_eq!(report_2.output, report_4.output);
}

#[test]
fn unproven_overflow_fails_closed_before_worker_dispatch() {
    let overflow_ir = lower_one("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(2147483647))");
    let err = ParallelCpuComputeBackend::new(4)
        .execute(&overflow_ir)
        .unwrap_err();
    assert!(matches!(
        err,
        ComputeExecutionError::NotEligible(blockers)
            if blockers.contains(&AdmissionBlocker::IntegerOverflowNotProven)
    ));
}

#[test]
fn repeated_runs_are_strictly_byte_identical() {
    let elements: Vec<i32> = (0..250).map(|x| x * 2 - 100).collect();
    let source = format!(
        "(numeric-buffer-map (lambda (x) (+ x 3)) #i32({}))",
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

#[test]
fn parallel_cpu_node_executor_integrates_with_heterogeneous_graph() {
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu("cpu-mc-4", ParallelCpuNodeExecutor::new(4));

    let node = PlanNode {
        id: NodeId(1),
        operation: ExecutionOperation::NumericBufferMap {
            function: lower_one("(lambda (x) (+ x 7))"),
            input: BufferId(0),
        },
        output: BufferId(1),
        dependencies: vec![],
        target: ExecutionTarget::Gpu {
            backend: "cpu-mc-4".into(),
        },
    };

    let graph = ExecutionGraph {
        inputs: vec![(
            BufferId(0),
            GraphValue::Buffer(BufferLiteral::I32(vec![1, 2, 3, 4, 5])),
        )],
        nodes: vec![node],
    };

    let result = executor.execute(&graph).unwrap();
    assert_eq!(
        result.buffer(BufferId(1)),
        Some(&BufferLiteral::I32(vec![8, 9, 10, 11, 12]))
    );
}

#[test]
fn diagnostic_performance_benchmark_on_large_buffer() {
    let count = 50_000;
    let raw: Vec<i32> = (0..count).map(|x| (x % 1000) as i32).collect();
    let ir = Ir::App {
        func: Box::new(Ir::Var("NUMERIC-BUFFER-MAP".into())),
        args: vec![
            lower_one("(lambda (x) (+ x 1))"),
            Ir::Buffer(BufferLiteral::I32(raw)),
        ],
    };

    let t1_start = Instant::now();
    let res_seq = CpuComputeBackend.execute(&ir).unwrap();
    let t1_dur = t1_start.elapsed();

    let t4_start = Instant::now();
    let res_par = ParallelCpuComputeBackend::new(4).execute(&ir).unwrap();
    let t4_dur = t4_start.elapsed();

    assert_eq!(res_seq, res_par);
    println!("50k i32 elements: 1-core={t1_dur:?}, 4-worker={t4_dur:?}");
}
