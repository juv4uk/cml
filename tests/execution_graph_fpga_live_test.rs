use std::env;

use cml::execution::{
    BufferId, ExecutionGraph, ExecutionOperation, ExecutionTarget, FpgaTransportNodeExecutor,
    GraphValue, HeterogeneousGraphExecutor, NodeId, PlanNode,
};
use cml::fpga_transport::{CommandFpgaTransport, FpgaJobV1};
use cml::ir::BufferLiteral;

#[test]
#[ignore = "requires a connected fpga-lisp board and a manual RESET press"]
fn execution_graph_runs_typed_buffer_add_on_the_live_fpga() {
    assert_eq!(env::var("CML_FPGA_LIVE").as_deref(), Ok("1"));
    let python = env::var("CML_FPGA_PYTHON").expect("set CML_FPGA_PYTHON to Windows py.exe");
    let bridge = env::var("CML_FPGA_BRIDGE_WINDOWS")
        .expect("set CML_FPGA_BRIDGE_WINDOWS to job_transport.py's Windows UNC path");
    let words = vec![0xd201_0000, 0xb000_0000];

    let transport = CommandFpgaTransport::new(
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
    );
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_fpga("fpga-lisp:com4", FpgaTransportNodeExecutor::new(transport));
    let output = BufferId(1);
    let result = executor
        .execute(&ExecutionGraph {
            inputs: vec![(
                BufferId(0),
                GraphValue::Buffer(BufferLiteral::I32(vec![3, 4])),
            )],
            nodes: vec![PlanNode {
                id: NodeId(1),
                operation: ExecutionOperation::FpgaProgramWithBufferInput {
                    job: FpgaJobV1 {
                        program_words: words,
                        register_inputs: vec![],
                        result_register: 2,
                    },
                    input: BufferId(0),
                    first_register: 0,
                },
                output,
                dependencies: vec![],
                target: ExecutionTarget::Fpga {
                    device: "fpga-lisp:com4".into(),
                },
            }],
        })
        .unwrap();

    assert_eq!(result.lisp_word(output), Some(7));
}
