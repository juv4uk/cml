use cml::execution::{
    BufferId, CpuGraphExecutor, ExecutionGraph, ExecutionOperation, ExecutionTarget,
    GraphExecutionError, NodeId, PlanNode,
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
        inputs: vec![(BufferId(0), BufferLiteral::I32(vec![1, 2, 3]))],
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
        inputs: vec![(BufferId(0), BufferLiteral::I32(vec![5]))],
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
        inputs: vec![(BufferId(0), BufferLiteral::I32(vec![1]))],
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
        inputs: vec![(BufferId(0), BufferLiteral::I32(vec![1]))],
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
        inputs: vec![(BufferId(0), BufferLiteral::I32(vec![1]))],
        nodes: vec![map_node(1, 0, 1, &[2], 1), map_node(2, 1, 2, &[1], 1)],
    };
    assert_eq!(
        CpuGraphExecutor.execute(&cycle),
        Err(GraphExecutionError::DependencyCycle)
    );
}
