#![cfg(feature = "gpu-cuda")]

use cml::execution::{
    BufferId, CudaNodeExecutor, ExecutionGraph, ExecutionOperation, ExecutionTarget, GraphValue,
    HeterogeneousGraphExecutor, NodeId, PlanNode,
};
use cml::gpu_cuda_runtime::discover_devices;
use cml::ir::{BufferLiteral, Ir, Params};

fn admitted_add_one() -> Ir {
    Ir::Lambda {
        params: Params::Fixed(vec!["X".to_string()]),
        body: Box::new(Ir::App {
            func: Box::new(Ir::Sid(sens::sens!(00001100))),
            args: vec![Ir::Var("X".to_string()), Ir::Int(1)],
        }),
    }
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn graph_dispatches_a_node_to_live_cuda_and_matches_cpu_semantics() {
    let devices = discover_devices().expect("CUDA discovery failed");
    let device = devices.first().expect("no CUDA device found");
    let backend = format!("cuda:{}", device.ordinal);
    let function = admitted_add_one();
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

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn linear_cuda_chain_keeps_semantics_and_materializes_every_graph_output() {
    let device = discover_devices()
        .expect("CUDA discovery failed")
        .into_iter()
        .next()
        .expect("no CUDA device found");
    let backend = format!("cuda:{}", device.ordinal);
    let add_one = admitted_add_one();
    let mut nodes = Vec::new();
    for index in 0..4u32 {
        nodes.push(PlanNode {
            id: NodeId(index + 1),
            operation: ExecutionOperation::NumericBufferMap {
                function: add_one.clone(),
                input: BufferId(index),
            },
            output: BufferId(index + 1),
            dependencies: if index == 0 {
                vec![]
            } else {
                vec![NodeId(index)]
            },
            target: ExecutionTarget::Gpu {
                backend: backend.clone(),
            },
        });
    }

    let graph = ExecutionGraph {
        inputs: vec![(
            BufferId(0),
            GraphValue::Buffer(BufferLiteral::I32(vec![1, 2, 3])),
        )],
        nodes,
    };
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu(
        backend,
        CudaNodeExecutor {
            device_ordinal: device.ordinal,
        },
    );

    let result = executor
        .execute(&graph)
        .expect("CUDA resident graph chain failed");
    assert_eq!(
        result.execution_order(),
        &[NodeId(1), NodeId(2), NodeId(3), NodeId(4)]
    );
    for step in 1..=4u32 {
        assert_eq!(
            result.buffer(BufferId(step)),
            Some(&BufferLiteral::I32(vec![
                1 + step as i32,
                2 + step as i32,
                3 + step as i32,
            ]))
        );
    }
}

#[test]
#[ignore = "requires a live NVIDIA CUDA device"]
fn selective_cuda_chain_materializes_only_requested_final_output() {
    let device = discover_devices()
        .expect("CUDA discovery failed")
        .into_iter()
        .next()
        .expect("no CUDA device found");
    let backend = format!("cuda:{}", device.ordinal);
    let add_one = admitted_add_one();
    let mut nodes = Vec::new();
    for index in 0..4u32 {
        nodes.push(PlanNode {
            id: NodeId(index + 1),
            operation: ExecutionOperation::NumericBufferMap {
                function: add_one.clone(),
                input: BufferId(index),
            },
            output: BufferId(index + 1),
            dependencies: if index == 0 {
                vec![]
            } else {
                vec![NodeId(index)]
            },
            target: ExecutionTarget::Gpu {
                backend: backend.clone(),
            },
        });
    }

    let graph = ExecutionGraph {
        inputs: vec![(
            BufferId(0),
            GraphValue::Buffer(BufferLiteral::I32(vec![1, 2, 3])),
        )],
        nodes,
    };
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu(
        backend,
        CudaNodeExecutor {
            device_ordinal: device.ordinal,
        },
    );

    let result = executor
        .execute_requested(&graph, &[BufferId(4)])
        .expect("CUDA selective resident graph chain failed");

    assert_eq!(
        result.execution_order(),
        &[NodeId(1), NodeId(2), NodeId(3), NodeId(4)]
    );
    assert_eq!(result.buffer(BufferId(0)), None);
    assert_eq!(result.buffer(BufferId(1)), None);
    assert_eq!(result.buffer(BufferId(2)), None);
    assert_eq!(result.buffer(BufferId(3)), None);
    assert_eq!(
        result.buffer(BufferId(4)),
        Some(&BufferLiteral::I32(vec![5, 6, 7]))
    );
}
