use cml::execution::{
    BufferId, CpuGraphExecutor, ExecutionGraph, ExecutionOperation, ExecutionTarget,
    GraphExecutionError, GraphValue, HeterogeneousGraphExecutor, NodeExecutor, NodeId, PlanNode,
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

use cml::execution::{ConcurrencyModel, ConcurrencyProfile};

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
