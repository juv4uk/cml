use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use cml::execution::{
    BufferId, ConcurrencyModel, ConcurrencyProfile, CpuGraphExecutor, ExecutionGraph,
    ExecutionMode, ExecutionOperation, ExecutionTarget, GraphExecutionError, GraphValue,
    HeterogeneousGraphExecutor, NodeExecutor, NodeId, PlanNode,
};
use cml::ir::{BufferLiteral, Ir};
use cml::{lower, parser};

fn map_function(offset: i32) -> Ir {
    let source = format!("(lambda (x) (+ x {offset}))");
    let expressions = parser::parse(&source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

fn map_node(id: u32, input: u32, output: u32, dependencies: &[u32], offset: i32) -> PlanNode {
    PlanNode {
        id: NodeId(id),
        operation: ExecutionOperation::NumericBufferMap {
            function: map_function(offset),
            input: BufferId(input),
        },
        output: BufferId(output),
        dependencies: dependencies.iter().copied().map(NodeId).collect(),
        target: ExecutionTarget::Cpu,
    }
}

#[test]
fn cpu_executor_runs_a_dependent_multi_node_graph() {
    let graph = ExecutionGraph {
        inputs: vec![(
            BufferId(0),
            GraphValue::Buffer(BufferLiteral::I32(vec![1, 2, 3])),
        )],
        nodes: vec![map_node(1, 0, 1, &[], 1), map_node(2, 1, 2, &[1], 10)],
    };

    let result = CpuGraphExecutor.execute(&graph).unwrap();
    assert_eq!(result.execution_order(), &[NodeId(1), NodeId(2)]);
    assert_eq!(
        result.buffer(BufferId(2)),
        Some(&BufferLiteral::I32(vec![12, 13, 14]))
    );
}

#[test]
fn dependency_order_is_not_source_order() {
    let graph = ExecutionGraph {
        inputs: vec![(BufferId(0), GraphValue::Buffer(BufferLiteral::I32(vec![5])))],
        nodes: vec![map_node(2, 1, 2, &[1], 2), map_node(1, 0, 1, &[], 1)],
    };

    let result = CpuGraphExecutor.execute(&graph).unwrap();
    assert_eq!(result.execution_order(), &[NodeId(1), NodeId(2)]);
    assert_eq!(
        result.buffer(BufferId(2)),
        Some(&BufferLiteral::I32(vec![8]))
    );
}

#[test]
fn accelerator_targets_fail_closed_without_an_executor() {
    let mut node = map_node(1, 0, 1, &[], 1);
    node.target = ExecutionTarget::Gpu {
        backend: "cuda:0".into(),
    };
    let graph = ExecutionGraph {
        inputs: vec![(BufferId(0), GraphValue::Buffer(BufferLiteral::I32(vec![1])))],
        nodes: vec![node],
    };

    assert!(matches!(
        CpuGraphExecutor.execute(&graph),
        Err(GraphExecutionError::TargetUnavailable {
            node: NodeId(1),
            target: ExecutionTarget::Gpu { .. }
        })
    ));
}

#[test]
fn malformed_dependencies_and_cycles_are_named_errors() {
    let missing = ExecutionGraph {
        inputs: vec![(BufferId(0), GraphValue::Buffer(BufferLiteral::I32(vec![1])))],
        nodes: vec![map_node(1, 0, 1, &[9], 1)],
    };
    assert_eq!(
        CpuGraphExecutor.execute(&missing),
        Err(GraphExecutionError::MissingDependency {
            node: NodeId(1),
            dependency: NodeId(9),
        })
    );

    let cycle = ExecutionGraph {
        inputs: vec![(BufferId(0), GraphValue::Buffer(BufferLiteral::I32(vec![1])))],
        nodes: vec![map_node(1, 0, 1, &[2], 1), map_node(2, 1, 2, &[1], 1)],
    };
    assert_eq!(
        CpuGraphExecutor.execute(&cycle),
        Err(GraphExecutionError::DependencyCycle)
    );
}

#[test]
fn data_edges_require_a_real_producer_and_an_explicit_dependency() {
    let no_producer = ExecutionGraph {
        inputs: vec![],
        nodes: vec![map_node(1, 99, 1, &[], 1)],
    };
    assert_eq!(
        CpuGraphExecutor.execute(&no_producer),
        Err(GraphExecutionError::MissingValueProducer {
            node: NodeId(1),
            value: BufferId(99),
        })
    );

    let implicit_source_order = ExecutionGraph {
        inputs: vec![(BufferId(0), GraphValue::Buffer(BufferLiteral::I32(vec![1])))],
        nodes: vec![map_node(1, 0, 1, &[], 1), map_node(2, 1, 2, &[], 1)],
    };
    assert_eq!(
        CpuGraphExecutor.execute(&implicit_source_order),
        Err(GraphExecutionError::MissingDataDependency {
            node: NodeId(2),
            value: BufferId(1),
            producer: NodeId(1),
        })
    );
}

struct PortableTestExecutor;

impl NodeExecutor for PortableTestExecutor {
    fn execute_map(&self, ir: &Ir) -> Result<BufferLiteral, String> {
        use cml::compute::{ComputeBackend, CpuComputeBackend};
        CpuComputeBackend
            .execute(ir)
            .map_err(|error| format!("{error:?}"))
    }
}

#[test]
fn registered_backend_executes_without_vendor_logic_in_the_graph() {
    let mut node = map_node(1, 0, 1, &[], 4);
    node.target = ExecutionTarget::Gpu {
        backend: "portable-test".into(),
    };
    let graph = ExecutionGraph {
        inputs: vec![(
            BufferId(0),
            GraphValue::Buffer(BufferLiteral::I32(vec![1, 2])),
        )],
        nodes: vec![node],
    };
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu("portable-test", PortableTestExecutor);

    let result = executor.execute(&graph).unwrap();
    assert_eq!(
        result.buffer(BufferId(1)),
        Some(&BufferLiteral::I32(vec![5, 6]))
    );
}

struct BoundedDummyExecutor {
    profile: ConcurrencyProfile,
}

impl NodeExecutor for BoundedDummyExecutor {
    fn execute_map(&self, _ir: &Ir) -> Result<BufferLiteral, String> {
        Ok(BufferLiteral::I32(vec![0]))
    }

    fn concurrency_profile(&self) -> ConcurrencyProfile {
        self.profile.clone()
    }
}

#[test]
fn concurrency_profile_defaults_and_explicit_descriptors() {
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu(
        "gpu-serial",
        BoundedDummyExecutor {
            profile: ConcurrencyProfile::serial("shared-gpu-resource"),
        },
    );
    executor.register_gpu(
        "gpu-bounded-2",
        BoundedDummyExecutor {
            profile: ConcurrencyProfile::bounded(2, "shared-gpu-resource"),
        },
    );

    let cpu_profile = executor
        .concurrency_profile(&ExecutionTarget::Cpu)
        .expect("cpu profile must exist");
    assert_eq!(cpu_profile.model, ConcurrencyModel::Reentrant);
    assert_eq!(cpu_profile.resource_key, "cpu");

    let serial_profile = executor
        .concurrency_profile(&ExecutionTarget::Gpu {
            backend: "gpu-serial".into(),
        })
        .expect("gpu-serial profile must exist");
    assert_eq!(serial_profile.model, ConcurrencyModel::Serial);
    assert_eq!(serial_profile.max_inflight, 1);
    assert_eq!(serial_profile.resource_key, "shared-gpu-resource");

    let bounded_profile = executor
        .concurrency_profile(&ExecutionTarget::Gpu {
            backend: "gpu-bounded-2".into(),
        })
        .expect("gpu-bounded profile must exist");
    assert_eq!(bounded_profile.model, ConcurrencyModel::Bounded(2));
    assert_eq!(bounded_profile.max_inflight, 2);
    assert_eq!(bounded_profile.resource_key, "shared-gpu-resource");
}

#[test]
fn cpu_executor_with_bound_greater_than_one_overlaps_independent_ready_nodes() {
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu(
        "cpu-pool-3",
        BoundedDummyExecutor {
            profile: ConcurrencyProfile::bounded(3, "cpu-pool"),
        },
    );

    let mut n1 = map_node(1, 0, 1, &[], 1);
    n1.target = ExecutionTarget::Gpu {
        backend: "cpu-pool-3".into(),
    };
    let mut n2 = map_node(2, 0, 2, &[], 2);
    n2.target = ExecutionTarget::Gpu {
        backend: "cpu-pool-3".into(),
    };
    let mut n3 = map_node(3, 0, 3, &[], 3);
    n3.target = ExecutionTarget::Gpu {
        backend: "cpu-pool-3".into(),
    };

    let ready = vec![&n1, &n2, &n3];
    let batches = executor.schedule_batches(&ready);

    assert_eq!(
        batches.len(),
        1,
        "all 3 nodes should overlap in a single batch"
    );
    assert_eq!(batches[0].len(), 3);
}

#[test]
fn explicitly_serial_executor_never_overlaps_multiple_ready_nodes() {
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu(
        "com-fpga",
        BoundedDummyExecutor {
            profile: ConcurrencyProfile::serial("com4-bus"),
        },
    );

    let mut n1 = map_node(1, 0, 1, &[], 1);
    n1.target = ExecutionTarget::Gpu {
        backend: "com-fpga".into(),
    };
    let mut n2 = map_node(2, 0, 2, &[], 2);
    n2.target = ExecutionTarget::Gpu {
        backend: "com-fpga".into(),
    };
    let mut n3 = map_node(3, 0, 3, &[], 3);
    n3.target = ExecutionTarget::Gpu {
        backend: "com-fpga".into(),
    };

    let ready = vec![&n1, &n2, &n3];
    let batches = executor.schedule_batches(&ready);

    assert_eq!(
        batches.len(),
        3,
        "serial executor must serialize each node into its own batch"
    );
    for batch in &batches {
        assert_eq!(batch.len(), 1);
    }
}

#[test]
fn two_logical_backend_names_sharing_one_physical_resource_cannot_exceed_bound() {
    let mut executor = HeterogeneousGraphExecutor::default();
    // Two distinct logical backend names pointing to the same physical device ("gpu-0") with max_inflight = 1
    executor.register_gpu(
        "compute-queue-a",
        BoundedDummyExecutor {
            profile: ConcurrencyProfile::bounded(1, "physical-gpu-0"),
        },
    );
    executor.register_gpu(
        "compute-queue-b",
        BoundedDummyExecutor {
            profile: ConcurrencyProfile::bounded(1, "physical-gpu-0"),
        },
    );

    let mut n1 = map_node(1, 0, 1, &[], 1);
    n1.target = ExecutionTarget::Gpu {
        backend: "compute-queue-a".into(),
    };
    let mut n2 = map_node(2, 0, 2, &[], 2);
    n2.target = ExecutionTarget::Gpu {
        backend: "compute-queue-b".into(),
    };

    let ready = vec![&n1, &n2];
    let batches = executor.schedule_batches(&ready);

    assert_eq!(
        batches.len(),
        2,
        "shared physical resource must prevent overlap even across distinct logical backend names"
    );
    assert_eq!(batches[0].len(), 1);
    assert_eq!(batches[1].len(), 1);
}

#[test]
fn unregistered_or_missing_concurrency_declaration_fails_closed_to_serial() {
    let executor = HeterogeneousGraphExecutor::default();
    // Target is unregistered; profile should be None
    assert_eq!(
        executor.concurrency_profile(&ExecutionTarget::Gpu {
            backend: "unregistered".into()
        }),
        None
    );

    let mut n1 = map_node(1, 0, 1, &[], 1);
    n1.target = ExecutionTarget::Gpu {
        backend: "unregistered".into(),
    };
    let mut n2 = map_node(2, 0, 2, &[], 2);
    n2.target = ExecutionTarget::Gpu {
        backend: "unregistered".into(),
    };

    let ready = vec![&n1, &n2];
    let batches = executor.schedule_batches(&ready);

    // Fail-closed behavior: each unregistered target is serialized safely, never given unlimited concurrency
    assert_eq!(batches.len(), 2);
}

struct OverlapWitnessExecutor {
    barrier: Arc<Barrier>,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    profile: ConcurrencyProfile,
}

impl NodeExecutor for OverlapWitnessExecutor {
    fn execute_map(&self, _ir: &Ir) -> Result<BufferLiteral, String> {
        let current = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(current, Ordering::SeqCst);
        self.barrier.wait();
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(BufferLiteral::I32(vec![42]))
    }

    fn concurrency_profile(&self) -> ConcurrencyProfile {
        self.profile.clone()
    }
}

#[test]
fn diamond_graph_proves_independent_nodes_overlap_physically() {
    let barrier = Arc::new(Barrier::new(2));
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));

    let witness_b = OverlapWitnessExecutor {
        barrier: Arc::clone(&barrier),
        active: Arc::clone(&active),
        max_active: Arc::clone(&max_active),
        profile: ConcurrencyProfile::reentrant("overlap-backend"),
    };

    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu("overlap-backend", witness_b);

    let mut node_b = map_node(2, 1, 2, &[1], 10);
    node_b.target = ExecutionTarget::Gpu {
        backend: "overlap-backend".into(),
    };
    let mut node_c = map_node(3, 1, 3, &[1], 20);
    node_c.target = ExecutionTarget::Gpu {
        backend: "overlap-backend".into(),
    };

    let graph = ExecutionGraph {
        inputs: vec![(
            BufferId(0),
            GraphValue::Buffer(BufferLiteral::I32(vec![1, 2])),
        )],
        nodes: vec![
            map_node(1, 0, 1, &[], 1),
            node_b,
            node_c,
            map_node(4, 2, 4, &[2, 3], 100),
        ],
    };

    let result = executor.execute(&graph).unwrap();
    assert_eq!(
        max_active.load(Ordering::SeqCst),
        2,
        "nodes B and C must have executed concurrently with physical overlap"
    );
    assert_eq!(
        result.execution_order(),
        &[NodeId(1), NodeId(2), NodeId(3), NodeId(4)]
    );
    assert_eq!(
        result.buffer(BufferId(4)),
        Some(&BufferLiteral::I32(vec![142]))
    );
}

struct SerialTrackerExecutor {
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
}

impl NodeExecutor for SerialTrackerExecutor {
    fn execute_map(&self, _ir: &Ir) -> Result<BufferLiteral, String> {
        let current = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(current, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(5));
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(BufferLiteral::I32(vec![10]))
    }

    fn concurrency_profile(&self) -> ConcurrencyProfile {
        ConcurrencyProfile::reentrant("serial-tracker")
    }
}

#[test]
fn dependency_chain_proves_dependent_nodes_never_overlap_illegally() {
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));

    let tracker = SerialTrackerExecutor {
        active: Arc::clone(&active),
        max_active: Arc::clone(&max_active),
    };

    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu("serial-tracker", tracker);

    let mut n1 = map_node(1, 0, 1, &[], 1);
    n1.target = ExecutionTarget::Gpu {
        backend: "serial-tracker".into(),
    };
    let mut n2 = map_node(2, 1, 2, &[1], 2);
    n2.target = ExecutionTarget::Gpu {
        backend: "serial-tracker".into(),
    };
    let mut n3 = map_node(3, 2, 3, &[2], 3);
    n3.target = ExecutionTarget::Gpu {
        backend: "serial-tracker".into(),
    };

    let graph = ExecutionGraph {
        inputs: vec![(BufferId(0), GraphValue::Buffer(BufferLiteral::I32(vec![1])))],
        nodes: vec![n1, n2, n3],
    };

    let result = executor.execute(&graph).unwrap();
    assert_eq!(
        max_active.load(Ordering::SeqCst),
        1,
        "dependent nodes in a linear chain must never overlap physically"
    );
    assert_eq!(result.execution_order(), &[NodeId(1), NodeId(2), NodeId(3)]);
}

struct FailingExecutor {
    message: String,
}

impl NodeExecutor for FailingExecutor {
    fn execute_map(&self, _ir: &Ir) -> Result<BufferLiteral, String> {
        Err(self.message.clone())
    }

    fn concurrency_profile(&self) -> ConcurrencyProfile {
        ConcurrencyProfile::reentrant("failing-target")
    }
}

#[test]
fn two_independent_failing_nodes_produce_deterministic_named_failure_policy() {
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu(
        "fail-2",
        FailingExecutor {
            message: "failure in node 2".into(),
        },
    );
    executor.register_gpu(
        "fail-3",
        FailingExecutor {
            message: "failure in node 3".into(),
        },
    );

    let mut n2 = map_node(2, 0, 2, &[], 1);
    n2.target = ExecutionTarget::Gpu {
        backend: "fail-2".into(),
    };
    let mut n3 = map_node(3, 0, 3, &[], 2);
    n3.target = ExecutionTarget::Gpu {
        backend: "fail-3".into(),
    };

    let graph = ExecutionGraph {
        inputs: vec![(BufferId(0), GraphValue::Buffer(BufferLiteral::I32(vec![1])))],
        nodes: vec![n2, n3],
    };

    // Repeat 20 times: regardless of physical completion timing,
    // the failure policy deterministically reports the error from the first node in batch order (node 2).
    for _ in 0..20 {
        let err = executor.execute(&graph).unwrap_err();
        match err {
            GraphExecutionError::Backend { node, message, .. } => {
                assert_eq!(node, NodeId(2));
                assert_eq!(message, "failure in node 2");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}

#[test]
fn repeated_runs_produce_same_logical_values_and_dependency_valid_publication() {
    let graph = ExecutionGraph {
        inputs: vec![(
            BufferId(0),
            GraphValue::Buffer(BufferLiteral::I32(vec![1, 2, 3])),
        )],
        nodes: vec![
            map_node(1, 0, 1, &[], 1),
            map_node(2, 1, 2, &[1], 10),
            map_node(3, 1, 3, &[1], 20),
            map_node(4, 2, 4, &[2, 3], 100),
        ],
    };

    let executor = HeterogeneousGraphExecutor::default();
    for _ in 0..50 {
        let result = executor.execute(&graph).unwrap();
        assert_eq!(
            result.execution_order(),
            &[NodeId(1), NodeId(2), NodeId(3), NodeId(4)]
        );
        assert_eq!(
            result.buffer(BufferId(4)),
            Some(&BufferLiteral::I32(vec![112, 113, 114]))
        );
        assert_eq!(
            result.buffer(BufferId(3)),
            Some(&BufferLiteral::I32(vec![22, 23, 24]))
        );
    }
}

#[test]
fn deliberate_dependency_removal_or_mutation_is_caught() {
    // Deliberate removal: node 2 uses buffer 1 produced by node 1, but omits node 1 from dependencies
    let bad_graph = ExecutionGraph {
        inputs: vec![(BufferId(0), GraphValue::Buffer(BufferLiteral::I32(vec![1])))],
        nodes: vec![
            map_node(1, 0, 1, &[], 1),
            map_node(2, 1, 2, &[], 10), // missing explicit dependency on node 1
        ],
    };
    let err = CpuGraphExecutor.execute(&bad_graph).unwrap_err();
    assert_eq!(
        err,
        GraphExecutionError::MissingDataDependency {
            node: NodeId(2),
            value: BufferId(1),
            producer: NodeId(1),
        }
    );
}

#[test]
fn sequential_compatibility_mode_gives_same_observations() {
    let graph = ExecutionGraph {
        inputs: vec![(
            BufferId(0),
            GraphValue::Buffer(BufferLiteral::I32(vec![5, 10, 15])),
        )],
        nodes: vec![
            map_node(1, 0, 1, &[], 2),
            map_node(2, 1, 2, &[1], 3),
            map_node(3, 1, 3, &[1], 4),
            map_node(4, 2, 4, &[2, 3], 5),
        ],
    };

    let executor = HeterogeneousGraphExecutor::default();
    let concurrent_result = executor.execute(&graph).unwrap();
    let sequential_result = executor.execute_sequential(&graph).unwrap();

    assert_eq!(
        concurrent_result.execution_order(),
        sequential_result.execution_order()
    );
    assert_eq!(
        concurrent_result.buffer(BufferId(1)),
        sequential_result.buffer(BufferId(1))
    );
    assert_eq!(
        concurrent_result.buffer(BufferId(2)),
        sequential_result.buffer(BufferId(2))
    );
    assert_eq!(
        concurrent_result.buffer(BufferId(3)),
        sequential_result.buffer(BufferId(3))
    );
    assert_eq!(
        concurrent_result.buffer(BufferId(4)),
        sequential_result.buffer(BufferId(4))
    );

    let explicit_concurrent = executor
        .execute_mode(&graph, ExecutionMode::Concurrent)
        .unwrap();
    let explicit_sequential = executor
        .execute_mode(&graph, ExecutionMode::Sequential)
        .unwrap();
    assert_eq!(
        explicit_concurrent.execution_order(),
        concurrent_result.execution_order()
    );
    assert_eq!(
        explicit_sequential.execution_order(),
        sequential_result.execution_order()
    );
}

#[test]
fn bounded_worker_pool_limits_in_flight_concurrency() {
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));

    struct BoundedPoolWitness {
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
    }

    impl NodeExecutor for BoundedPoolWitness {
        fn execute_map(&self, _ir: &Ir) -> Result<BufferLiteral, String> {
            let cur = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(cur, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(10));
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(BufferLiteral::I32(vec![1]))
        }

        fn concurrency_profile(&self) -> ConcurrencyProfile {
            ConcurrencyProfile::reentrant("pool-device")
        }
    }

    let mut executor = HeterogeneousGraphExecutor::default().with_max_workers(2);
    executor.register_gpu(
        "pool-device",
        BoundedPoolWitness {
            active: Arc::clone(&active),
            max_active: Arc::clone(&max_active),
        },
    );

    let mut nodes = Vec::new();
    for i in 1..=4 {
        let mut n = map_node(i, 0, i, &[], 1);
        n.target = ExecutionTarget::Gpu {
            backend: "pool-device".into(),
        };
        nodes.push(n);
    }

    let graph = ExecutionGraph {
        inputs: vec![(BufferId(0), GraphValue::Buffer(BufferLiteral::I32(vec![1])))],
        nodes,
    };

    let result = executor.execute(&graph).unwrap();
    assert_eq!(result.execution_order().len(), 4);
    assert!(
        max_active.load(Ordering::SeqCst) <= 2,
        "worker pool limit of 2 must not be exceeded (observed {})",
        max_active.load(Ordering::SeqCst)
    );
}

#[test]
fn failure_prevents_publication_of_invalid_downstream_work() {
    let downstream_executed = Arc::new(AtomicBool::new(false));

    struct DownstreamWitness {
        executed: Arc<AtomicBool>,
    }

    impl NodeExecutor for DownstreamWitness {
        fn execute_map(&self, _ir: &Ir) -> Result<BufferLiteral, String> {
            self.executed.store(true, Ordering::SeqCst);
            Ok(BufferLiteral::I32(vec![99]))
        }
    }

    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu(
        "fail-node",
        FailingExecutor {
            message: "boom".into(),
        },
    );
    executor.register_gpu(
        "downstream-witness",
        DownstreamWitness {
            executed: Arc::clone(&downstream_executed),
        },
    );

    let mut n1 = map_node(1, 0, 1, &[], 1);
    n1.target = ExecutionTarget::Gpu {
        backend: "fail-node".into(),
    };
    let mut n2 = map_node(2, 1, 2, &[1], 2);
    n2.target = ExecutionTarget::Gpu {
        backend: "downstream-witness".into(),
    };

    let graph = ExecutionGraph {
        inputs: vec![(BufferId(0), GraphValue::Buffer(BufferLiteral::I32(vec![1])))],
        nodes: vec![n1, n2],
    };

    let err = executor.execute(&graph).unwrap_err();
    assert!(matches!(
        err,
        GraphExecutionError::Backend {
            node: NodeId(1),
            ..
        }
    ));
    assert!(
        !downstream_executed.load(Ordering::SeqCst),
        "downstream node must never execute after failure of prerequisite"
    );
}
