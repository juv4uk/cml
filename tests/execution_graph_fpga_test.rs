use cml::execution::{
    BufferId, ExecutionGraph, ExecutionOperation, ExecutionTarget, FpgaTransportNodeExecutor,
    GraphValue, HeterogeneousGraphExecutor, NodeId, PlanNode,
};
use cml::fpga_transport::{FpgaJobV1, FpgaProtocolError, FpgaResultV1, FpgaTransport};
use cml::ir::BufferLiteral;

struct BoardWitness {
    result: FpgaResultV1,
}

impl FpgaTransport for BoardWitness {
    fn execute(&mut self, job: &FpgaJobV1) -> Result<FpgaResultV1, FpgaProtocolError> {
        assert_eq!(job.bootloader_frame().unwrap(), vec![1, 0, 0, 0, 0, 0]);
        assert_eq!(job.result_query().unwrap(), [0x01, 9]);
        Ok(self.result)
    }
}

#[test]
fn graph_preserves_an_fpga_tagged_word_without_calling_it_a_buffer() {
    let job = FpgaJobV1 {
        program_words: vec![0],
        register_inputs: vec![],
        result_register: 9,
    };
    let graph = ExecutionGraph {
        inputs: vec![],
        nodes: vec![PlanNode {
            id: NodeId(1),
            operation: ExecutionOperation::FpgaProgram { job },
            output: BufferId(1),
            dependencies: vec![],
            target: ExecutionTarget::Fpga {
                device: "board-witness".into(),
            },
        }],
    };
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_fpga(
        "board-witness",
        FpgaTransportNodeExecutor::new(BoardWitness {
            result: FpgaResultV1::from_monitor_words(7, 0),
        }),
    );

    let result = executor.execute(&graph).unwrap();
    assert_eq!(result.value(BufferId(1)), Some(&GraphValue::LispWord(7)));
    assert_eq!(result.lisp_word(BufferId(1)), Some(7));
    assert_eq!(result.buffer(BufferId(1)), None);
}

#[test]
fn graph_does_not_publish_a_word_when_fpga_reports_an_error() {
    let graph = ExecutionGraph {
        inputs: vec![],
        nodes: vec![PlanNode {
            id: NodeId(1),
            operation: ExecutionOperation::FpgaProgram {
                job: FpgaJobV1 {
                    program_words: vec![0],
                    register_inputs: vec![],
                    result_register: 9,
                },
            },
            output: BufferId(1),
            dependencies: vec![],
            target: ExecutionTarget::Fpga {
                device: "board-witness".into(),
            },
        }],
    };
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_fpga(
        "board-witness",
        FpgaTransportNodeExecutor::new(BoardWitness {
            result: FpgaResultV1::from_monitor_words(7, 0x1000 | 42),
        }),
    );

    assert!(executor.execute(&graph).is_err());
}

struct InputWitness;

impl FpgaTransport for InputWitness {
    fn execute(&mut self, job: &FpgaJobV1) -> Result<FpgaResultV1, FpgaProtocolError> {
        assert_eq!(
            job.register_inputs
                .iter()
                .map(|input| (input.register, input.tagged_word))
                .collect::<Vec<_>>(),
            vec![(0, 3), (1, 4)]
        );
        Ok(FpgaResultV1::from_monitor_words(7, 0))
    }
}

#[test]
fn graph_materializes_an_explicit_buffer_edge_into_fpga_register_inputs() {
    let graph = ExecutionGraph {
        inputs: vec![(
            BufferId(10),
            GraphValue::Buffer(BufferLiteral::I32(vec![3, 4])),
        )],
        nodes: vec![PlanNode {
            id: NodeId(1),
            operation: ExecutionOperation::FpgaProgramWithBufferInput {
                job: FpgaJobV1 {
                    program_words: vec![0],
                    register_inputs: vec![],
                    result_register: 2,
                },
                input: BufferId(10),
                first_register: 0,
            },
            output: BufferId(11),
            dependencies: vec![],
            target: ExecutionTarget::Fpga {
                device: "input-witness".into(),
            },
        }],
    };
    let mut executor = HeterogeneousGraphExecutor::default();
    executor.register_fpga("input-witness", FpgaTransportNodeExecutor::new(InputWitness));
    let result = executor.execute(&graph).unwrap();
    assert_eq!(result.lisp_word(BufferId(11)), Some(7));
}
