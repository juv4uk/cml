#![cfg(feature = "gpu-cuda")]

use cml::execution::{
    BufferId, CudaNodeExecutor, ExecutionGraph, ExecutionOperation, ExecutionTarget, GraphValue,
    HeterogeneousGraphExecutor, NodeId, PlanNode,
};
use cml::gpu_cuda_runtime::discover_devices;
use cml::ir::{BufferLiteral, Ir};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn graph_dispatches_a_node_to_live_cuda_and_matches_cpu_semantics() {
    let devices = discover_devices().expect("CUDA discovery failed");
    let device = devices.first().expect("no CUDA device found");
    let backend = format!("cuda:{}", device.ordinal);
    let function = lower_one("(lambda (x) (+ x 1))");
    let graph = ExecutionGraph {
        inputs: vec![(
            BufferId(0),
            GraphValue::Buffer(BufferLiteral::I32(vec![1, 2, 3])),
        )],
        nodes: vec![PlanNode {
            id: NodeId(1),
            operation: ExecutionOperation::NumericBufferMap {
                function,
                input: BufferId(0),
            },
            output: BufferId(1),
            dependencies: vec![],
            target: ExecutionTarget::Gpu {
                backend: backend.clone(),
            },
        }],
    };
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu(
        backend,
        CudaNodeExecutor {
            device_ordinal: device.ordinal,
        },
    );

    let result = executor.execute(&graph).expect("CUDA graph failed");
    assert_eq!(
        result.buffer(BufferId(1)),
        Some(&BufferLiteral::I32(vec![2, 3, 4]))
    );
}
