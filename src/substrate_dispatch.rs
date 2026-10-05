//! Fail-closed routing from admitted CML work to physical execution substrates.
//!
//! The dispatcher does not define language meaning. It joins the existing
//! placement policy to the heterogeneous executor and returns the placement
//! provenance together with the physical result.

use crate::compute::{BulkOperation, analyze};
use crate::execution::{
    BufferId, ExecutionGraph, ExecutionOperation, ExecutionTarget, FpgaProgramNodeExecutor,
    GraphExecutionError, GraphValue, HeterogeneousGraphExecutor, NodeExecutor, NodeId,
    ParallelCpuNodeExecutor, PlanNode,
};
use crate::fpga_transport::FpgaJobV1;
use crate::gpu_worker_client::SharedCudaWorkerNodeExecutor;
use crate::ir::{BufferLiteral, Ir};
use crate::placement::{
    AcceleratorCapabilityEvidence, PlacementConfig, PlacementDecision, PlacementInputs,
    PlacementOverride, PlacementRejection, place,
};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct DispatchOutcome<T> {
    pub decision: Option<PlacementDecision>,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    Placement(PlacementRejection),
    Execution(GraphExecutionError),
    NotNumericBufferMap,
    FpgaNumericMapNotAdmitted { device: String },
    MissingNumericOutput,
    MissingFpgaOutput,
}

impl From<PlacementRejection> for DispatchError {
    fn from(value: PlacementRejection) -> Self {
        Self::Placement(value)
    }
}

impl From<GraphExecutionError> for DispatchError {
    fn from(value: GraphExecutionError) -> Self {
        Self::Execution(value)
    }
}

/// One mechanism-level dispatcher shared by local runners and host tools.
///
/// Automatic placement currently compares CPU and parity-proven GPU mechanisms
/// using `crate::placement`. FPGA program jobs use the explicit FPGA lane.
/// Numeric-map -> FPGA remains fail-closed until comparable FPGA eligibility and
/// cost evidence is defined.
pub struct SubstrateDispatcher {
    executor: HeterogeneousGraphExecutor,
    config: PlacementConfig,
    available_cpu_workers: usize,
    accelerator_evidence: Vec<AcceleratorCapabilityEvidence>,
    gpu_input_resident: bool,
    gpu_output_can_remain_resident: bool,
}

impl SubstrateDispatcher {
    pub fn new(config: PlacementConfig, available_cpu_workers: usize) -> Self {
        Self {
            executor: HeterogeneousGraphExecutor::default(),
            config,
            available_cpu_workers: available_cpu_workers.max(1),
            accelerator_evidence: Vec::new(),
            gpu_input_resident: false,
            gpu_output_can_remain_resident: false,
        }
    }

    pub fn with_accelerator_evidence(
        mut self,
        evidence: Vec<AcceleratorCapabilityEvidence>,
    ) -> Self {
        self.accelerator_evidence = evidence;
        self
    }

    pub fn set_accelerator_evidence(&mut self, evidence: Vec<AcceleratorCapabilityEvidence>) {
        self.accelerator_evidence = evidence;
    }

    pub fn set_gpu_residency(&mut self, input_resident: bool, output_can_remain_resident: bool) {
        self.gpu_input_resident = input_resident;
        self.gpu_output_can_remain_resident = output_can_remain_resident;
    }

    pub fn register_gpu(
        &mut self,
        backend: impl Into<String>,
        executor: impl NodeExecutor + 'static,
    ) {
        self.executor.register_gpu(backend, executor);
    }

    /// Register the persistent shared CUDA worker without granting semantic
    /// authority. `parity_proven` is caller evidence for the admitted workload,
    /// not a property inferred from the socket existing.
    pub fn register_shared_cuda_worker(
        &mut self,
        backend: impl Into<String>,
        socket: impl Into<PathBuf>,
        parity_proven: bool,
    ) {
        let backend = backend.into();
        self.executor
            .register_gpu(backend.clone(), SharedCudaWorkerNodeExecutor::new(socket));
        let evidence = if parity_proven {
            AcceleratorCapabilityEvidence::proven_gpu(backend)
        } else {
            AcceleratorCapabilityEvidence::unproven_gpu(backend)
        };
        self.accelerator_evidence
            .retain(|current| current.backend_tag != evidence.backend_tag);
        self.accelerator_evidence.push(evidence);
    }

    pub fn register_fpga(
        &mut self,
        device: impl Into<String>,
        executor: impl FpgaProgramNodeExecutor + 'static,
    ) {
        self.executor.register_fpga(device, executor);
    }

    /// Analyze, place, and execute one admitted numeric-buffer map.
    ///
    /// Automatic routing is CPU/CUDA only today. An explicit FPGA override is
    /// rejected before execution because a NumericBufferMap has no admitted
    /// FPGA lowering in the current graph contract.
    pub fn execute_numeric_map(
        &mut self,
        ir: &Ir,
        override_: Option<&PlacementOverride>,
    ) -> Result<DispatchOutcome<BufferLiteral>, DispatchError> {
        let analysis = analyze(ir);
        let region = analysis
            .region
            .as_ref()
            .filter(|region| region.operation == BulkOperation::Map)
            .ok_or(DispatchError::NotNumericBufferMap)?;
        let input = match &region.input {
            Ir::Buffer(buffer) => buffer.clone(),
            _ => return Err(DispatchError::NotNumericBufferMap),
        };
        let element_count = match &input {
            BufferLiteral::I32(values) => values.len(),
            BufferLiteral::F32(values) => values.len(),
        };

        let placement_inputs = PlacementInputs {
            element_count: Some(element_count),
            analysis: analysis.clone(),
            available_cpu_workers: self.available_cpu_workers,
            accelerator_evidence: self.accelerator_evidence.clone(),
            gpu_input_resident: self.gpu_input_resident,
            gpu_output_can_remain_resident: self.gpu_output_can_remain_resident,
        };
        let decision = place(&placement_inputs, &self.config, override_)?;

        if let ExecutionTarget::Fpga { device } = &decision.target {
            return Err(DispatchError::FpgaNumericMapNotAdmitted {
                device: device.clone(),
            });
        }

        self.ensure_parallel_cpu_executor(&decision);

        let graph = ExecutionGraph {
            inputs: vec![(BufferId(0), GraphValue::Buffer(input))],
            nodes: vec![PlanNode {
                id: NodeId(1),
                operation: ExecutionOperation::NumericBufferMap {
                    function: region.function.clone(),
                    input: BufferId(0),
                },
                output: BufferId(1),
                dependencies: vec![],
                target: decision.target.clone(),
            }],
        };
        let result = self.executor.execute(&graph)?;
        let value = result
            .buffer(BufferId(1))
            .cloned()
            .ok_or(DispatchError::MissingNumericOutput)?;
        Ok(DispatchOutcome {
            decision: Some(decision),
            value,
        })
    }

    /// Route an already-admitted FPGA program job to one named physical FPGA
    /// executor. No automatic cross-substrate cost claim is made here.
    pub fn execute_fpga_job(
        &self,
        device: impl Into<String>,
        job: FpgaJobV1,
    ) -> Result<DispatchOutcome<u32>, DispatchError> {
        let device = device.into();
        let graph = ExecutionGraph {
            inputs: vec![],
            nodes: vec![PlanNode {
                id: NodeId(1),
                operation: ExecutionOperation::FpgaProgram { job },
                output: BufferId(1),
                dependencies: vec![],
                target: ExecutionTarget::Fpga {
                    device: device.clone(),
                },
            }],
        };
        let result = self.executor.execute(&graph)?;
        let value = result
            .lisp_word(BufferId(1))
            .ok_or(DispatchError::MissingFpgaOutput)?;
        Ok(DispatchOutcome {
            decision: None,
            value,
        })
    }

    fn ensure_parallel_cpu_executor(&mut self, decision: &PlacementDecision) {
        let ExecutionTarget::Gpu { backend } = &decision.target else {
            return;
        };
        let Some(workers) = backend
            .strip_prefix("cpu-parallel-")
            .and_then(|value| value.parse::<usize>().ok())
        else {
            return;
        };
        self.executor
            .register_gpu(backend.clone(), ParallelCpuNodeExecutor::new(workers));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::ConcurrencyProfile;
    use crate::ir::Params;
    use crate::placement::{AcceleratorCapabilityEvidence, PlacementCostProfile};

    fn add_map(count: usize, offset: i64) -> Ir {
        let values = (0..count).map(|value| value as i32).collect();
        Ir::App {
            func: Box::new(Ir::Sid(sens::sens!(01011001))),
            args: vec![
                Ir::Lambda {
                    params: Params::Fixed(vec!["X".into()]),
                    body: Box::new(Ir::App {
                        func: Box::new(Ir::Sid(sens::sens!(00001100))),
                        args: vec![Ir::Var("X".into()), Ir::Int(offset)],
                    }),
                },
                Ir::Buffer(BufferLiteral::I32(values)),
            ],
        }
    }

    #[test]
    fn small_admitted_map_routes_to_cpu() {
        let mut dispatcher = SubstrateDispatcher::new(PlacementConfig::default(), 4);
        let outcome = dispatcher.execute_numeric_map(&add_map(16, 1), None).unwrap();
        assert_eq!(outcome.value, BufferLiteral::I32((1..=16).collect()));
        assert!(matches!(
            outcome.decision.unwrap().target,
            ExecutionTarget::Cpu
        ));
    }

    #[test]
    fn large_map_registers_and_executes_parallel_cpu_lane() {
        let mut dispatcher = SubstrateDispatcher::new(PlacementConfig::default(), 4);
        let outcome = dispatcher
            .execute_numeric_map(&add_map(5_000, 1), None)
            .unwrap();
        assert_eq!(
            outcome.value,
            BufferLiteral::I32((1..=5_000).collect::<Vec<i32>>())
        );
        assert!(matches!(
            outcome.decision.unwrap().target,
            ExecutionTarget::Gpu { ref backend } if backend == "cpu-parallel-4"
        ));
    }

    #[test]
    fn calibrated_gpu_choice_uses_registered_executor() {
        let config = PlacementConfig {
            multicore_element_threshold: 4_096,
            cost_profile: PlacementCostProfile {
                cpu_dispatch_cost: 1_000_000,
                cpu_element_cost: 1_000,
                cpu_operation_cost: 1_000,
                gpu_launch_cost: 0,
                gpu_element_cost: 0,
                gpu_operation_cost: 0,
                h2d_byte_cost: 0,
                dtoh_byte_cost: 0,
            },
        };
        let mut dispatcher = SubstrateDispatcher::new(config, 4).with_accelerator_evidence(vec![
            AcceleratorCapabilityEvidence::proven_gpu("test-gpu"),
        ]);
        dispatcher.register_gpu("test-gpu", ParallelCpuNodeExecutor::new(1));

        let outcome = dispatcher
            .execute_numeric_map(&add_map(128, 2), None)
            .unwrap();
        assert_eq!(
            outcome.value,
            BufferLiteral::I32((2..130).collect::<Vec<i32>>())
        );
        assert!(matches!(
            outcome.decision.unwrap().target,
            ExecutionTarget::Gpu { ref backend } if backend == "test-gpu"
        ));
    }

    #[test]
    fn selected_but_unregistered_gpu_fails_closed() {
        let config = PlacementConfig {
            multicore_element_threshold: 4_096,
            cost_profile: PlacementCostProfile {
                cpu_dispatch_cost: 1_000_000,
                cpu_element_cost: 1_000,
                cpu_operation_cost: 1_000,
                gpu_launch_cost: 0,
                gpu_element_cost: 0,
                gpu_operation_cost: 0,
                h2d_byte_cost: 0,
                dtoh_byte_cost: 0,
            },
        };
        let mut dispatcher = SubstrateDispatcher::new(config, 4).with_accelerator_evidence(vec![
            AcceleratorCapabilityEvidence::proven_gpu("missing-gpu"),
        ]);
        let error = dispatcher
            .execute_numeric_map(&add_map(128, 1), None)
            .unwrap_err();
        assert!(matches!(
            error,
            DispatchError::Execution(GraphExecutionError::TargetUnavailable {
                target: ExecutionTarget::Gpu { ref backend },
                ..
            }) if backend == "missing-gpu"
        ));
    }

    struct FakeFpga;

    impl FpgaProgramNodeExecutor for FakeFpga {
        fn execute_program(&self, _job: &FpgaJobV1) -> Result<u32, String> {
            Ok(0x1234)
        }

        fn concurrency_profile(&self) -> ConcurrencyProfile {
            ConcurrencyProfile::serial("fake-fpga")
        }
    }

    #[test]
    fn explicit_fpga_program_routes_through_registered_executor() {
        let mut dispatcher = SubstrateDispatcher::new(PlacementConfig::default(), 4);
        dispatcher.register_fpga("fpga-test", FakeFpga);
        let outcome = dispatcher
            .execute_fpga_job(
                "fpga-test",
                FpgaJobV1 {
                    program_words: vec![0],
                    register_inputs: vec![],
                    result_register: 0,
                },
            )
            .unwrap();
        assert_eq!(outcome.value, 0x1234);
        assert!(outcome.decision.is_none());
    }

    #[test]
    fn numeric_map_to_fpga_is_named_fail_closed_until_lowering_exists() {
        let mut dispatcher = SubstrateDispatcher::new(PlacementConfig::default(), 4)
            .with_accelerator_evidence(vec![AcceleratorCapabilityEvidence::live_fpga(
                "fpga-test",
            )]);
        let error = dispatcher
            .execute_numeric_map(
                &add_map(16, 1),
                Some(&PlacementOverride::Fpga {
                    device_tag: "fpga-test".into(),
                }),
            )
            .unwrap_err();
        assert_eq!(
            error,
            DispatchError::FpgaNumericMapNotAdmitted {
                device: "fpga-test".into()
            }
        );
    }
}
