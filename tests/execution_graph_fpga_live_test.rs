use std::env;
use std::fs;

use cml::execution::{
    BufferId, ExecutionGraph, ExecutionOperation, ExecutionTarget, FpgaTransportNodeExecutor,
    HeterogeneousGraphExecutor, NodeId, PlanNode,
};
use cml::fpga_transport::{CommandFpgaTransport, FpgaJobV1};

#[test]
#[ignore = "requires a connected fpga-lisp board and a manual RESET press"]
fn execution_graph_runs_bootstrap_add_on_the_live_fpga() {
    assert_eq!(env::var("CML_FPGA_LIVE").as_deref(), Ok("1"));
    let python = env::var("CML_FPGA_PYTHON").expect("set CML_FPGA_PYTHON to Windows py.exe");
    let bridge = env::var("CML_FPGA_BRIDGE_WINDOWS")
        .expect("set CML_FPGA_BRIDGE_WINDOWS to job_transport.py's Windows UNC path");
    let binary = fs::read("../fpga-lisp/bootstrap_add_demo.bin")
        .expect("assemble ../fpga-lisp/bootstrap_add_demo.bin first");
    assert_eq!(binary.len() % 4, 0);
    let words = binary
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect();

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
            inputs: vec![],
            nodes: vec![PlanNode {
                id: NodeId(1),
                operation: ExecutionOperation::FpgaProgram {
                    job: FpgaJobV1 {
                        program_words: words,
                        result_register: 9,
                    },
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
