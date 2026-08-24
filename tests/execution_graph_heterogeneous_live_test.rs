#![cfg(feature = "gpu-cuda")]

use std::env;

use cml::execution::{
    BufferId, CudaNodeExecutor, ExecutionGraph, ExecutionOperation, ExecutionTarget,
    FpgaTransportNodeExecutor, GraphValue, HeterogeneousGraphExecutor, NodeId, PlanNode,
};
use cml::fpga_transport::{CommandFpgaTransport, FpgaJobV1};
use cml::gpu_cuda_runtime::discover_devices;
use cml::ir::{BufferLiteral, Ir};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

#[test]
#[ignore = "requires live CUDA, a connected fpga-lisp board, and a manual RESET press"]
fn one_graph_orders_live_cpu_cuda_and_fpga_execution() {
    assert_eq!(env::var("CML_HETEROGENEOUS_LIVE").as_deref(), Ok("1"));
    let python = env::var("CML_FPGA_PYTHON").expect("set CML_FPGA_PYTHON to Windows py.exe");
    let bridge = env::var("CML_FPGA_BRIDGE_WINDOWS")
        .expect("set CML_FPGA_BRIDGE_WINDOWS to job_transport.py's Windows UNC path");
    let program_words = vec![0xd201_0000, 0xb000_0000];

    let cuda_device = discover_devices()
        .expect("CUDA discovery failed")
        .into_iter()
        .next()
        .expect("no CUDA device found");
    let cuda_backend = format!("cuda:{}", cuda_device.ordinal);
    let fpga_device = "fpga-lisp:com4".to_owned();
    let add_one = lower_one("(lambda (x) (+ x 1))");

    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_gpu(
        cuda_backend.clone(),
        CudaNodeExecutor {
            device_ordinal: cuda_device.ordinal,
        },
    );
    executor.register_fpga(
        fpga_device.clone(),
        FpgaTransportNodeExecutor::new(CommandFpgaTransport::new(
            python,
            vec![
                "-3".into(),
                bridge,
                "--port".into(),
                "COM4".into(),
                "--reset-wait".into(),
                "10".into(),
                "--halt-wait".into(),
                "3".into(),
            ],
        )),
    );

    let result = executor
        .execute(&ExecutionGraph {
            inputs: vec![(
                BufferId(0),
                GraphValue::Buffer(BufferLiteral::I32(vec![1, 2, 3])),
            )],
            nodes: vec![
                PlanNode {
                    id: NodeId(1),
                    operation: ExecutionOperation::NumericBufferMap {
                        function: add_one.clone(),
                        input: BufferId(0),
                    },
                    output: BufferId(1),
                    dependencies: vec![],
                    target: ExecutionTarget::Cpu,
                },
                PlanNode {
                    id: NodeId(2),
                    operation: ExecutionOperation::NumericBufferMap {
                        function: add_one,
                        input: BufferId(1),
                    },
                    output: BufferId(2),
                    dependencies: vec![NodeId(1)],
                    target: ExecutionTarget::Gpu {
                        backend: cuda_backend,
                    },
                },
                PlanNode {
                    id: NodeId(3),
                    operation: ExecutionOperation::FpgaProgramWithBufferInput {
                        job: FpgaJobV1 {
                            program_words,
                            register_inputs: vec![],
                            result_register: 2,
                        },
                        input: BufferId(2),
                        first_register: 0,
                    },
                    output: BufferId(3),
                    dependencies: vec![NodeId(2)],
                    target: ExecutionTarget::Fpga {
                        device: fpga_device,
                    },
                },
            ],
        })
        .unwrap();

    assert_eq!(result.execution_order(), &[NodeId(1), NodeId(2), NodeId(3)]);
    assert_eq!(
        result.buffer(BufferId(1)),
        Some(&BufferLiteral::I32(vec![2, 3, 4]))
    );
    assert_eq!(
        result.buffer(BufferId(2)),
        Some(&BufferLiteral::I32(vec![3, 4, 5]))
    );
    assert_eq!(result.lisp_word(BufferId(3)), Some(7));
}
