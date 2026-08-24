//! Device-neutral execution graph and CPU reference executor.
//!
//! M0 deliberately executes only CPU nodes. GPU and FPGA targets are
//! representable so the graph contract does not need to change later, but
//! they fail closed until a live executor is registered.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::compute::{ComputeBackend, ComputeExecutionError, CpuComputeBackend};
use crate::fpga_transport::{FpgaJobExecutor, FpgaJobV1, FpgaTransport};
use crate::ir::{BufferLiteral, Ir};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionTarget {
    Cpu,
    Gpu { backend: String },
    Fpga { device: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionOperation {
    NumericBufferMap { function: Ir, input: BufferId },
    FpgaProgram { job: FpgaJobV1 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphValue {
    Buffer(BufferLiteral),
    LispWord(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanNode {
    pub id: NodeId,
    pub operation: ExecutionOperation,
    pub output: BufferId,
    pub dependencies: Vec<NodeId>,
    pub target: ExecutionTarget,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExecutionGraph {
    pub inputs: Vec<(BufferId, GraphValue)>,
    pub nodes: Vec<PlanNode>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExecutionResult {
    values: HashMap<BufferId, GraphValue>,
    execution_order: Vec<NodeId>,
}

impl ExecutionResult {
    pub fn buffer(&self, id: BufferId) -> Option<&BufferLiteral> {
        match self.values.get(&id) {
            Some(GraphValue::Buffer(buffer)) => Some(buffer),
            _ => None,
        }
    }

    pub fn lisp_word(&self, id: BufferId) -> Option<u32> {
        match self.values.get(&id) {
            Some(GraphValue::LispWord(word)) => Some(*word),
            _ => None,
        }
    }

    pub fn value(&self, id: BufferId) -> Option<&GraphValue> {
        self.values.get(&id)
    }

    pub fn execution_order(&self) -> &[NodeId] {
        &self.execution_order
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphExecutionError {
    DuplicateInput(BufferId),
    DuplicateNode(NodeId),
    DuplicateOutput(BufferId),
    MissingDependency {
        node: NodeId,
        dependency: NodeId,
    },
    DependencyCycle,
    MissingInput {
        node: NodeId,
        buffer: BufferId,
    },
    InputKindMismatch {
        node: NodeId,
        value: BufferId,
    },
    TargetUnavailable {
        node: NodeId,
        target: ExecutionTarget,
    },
    Compute {
        node: NodeId,
        source: ComputeExecutionError,
    },
    Backend {
        node: NodeId,
        target: ExecutionTarget,
        message: String,
    },
}

/// One physical executor for one graph target. The graph owns semantic
/// admission and logical buffers; implementations own device transfer,
/// dispatch, and readback.
pub trait NodeExecutor {
    fn execute_map(&self, ir: &Ir) -> Result<BufferLiteral, String>;
}

pub trait FpgaProgramNodeExecutor {
    fn execute_program(&self, job: &FpgaJobV1) -> Result<u32, String>;
}

#[derive(Default)]
pub struct HeterogeneousGraphExecutor {
    gpu: HashMap<String, Box<dyn NodeExecutor>>,
    fpga: HashMap<String, Box<dyn FpgaProgramNodeExecutor>>,
}

impl HeterogeneousGraphExecutor {
    pub fn register_gpu(
        &mut self,
        backend: impl Into<String>,
        executor: impl NodeExecutor + 'static,
    ) {
        self.gpu.insert(backend.into(), Box::new(executor));
    }

    pub fn register_fpga(
        &mut self,
        device: impl Into<String>,
        executor: impl FpgaProgramNodeExecutor + 'static,
    ) {
        self.fpga.insert(device.into(), Box::new(executor));
    }

    pub fn execute(&self, graph: &ExecutionGraph) -> Result<ExecutionResult, GraphExecutionError> {
        validate_graph(graph)?;
        let mut values: HashMap<_, _> = graph.inputs.iter().cloned().collect();
        let mut completed = HashSet::new();
        let mut execution_order = Vec::with_capacity(graph.nodes.len());

        while completed.len() < graph.nodes.len() {
            let Some(node) = graph.nodes.iter().find(|node| {
                !completed.contains(&node.id)
                    && node
                        .dependencies
                        .iter()
                        .all(|dependency| completed.contains(dependency))
            }) else {
                return Err(GraphExecutionError::DependencyCycle);
            };

            let output = match &node.operation {
                ExecutionOperation::NumericBufferMap { function, input } => {
                    let input_value =
                        values.get(input).ok_or(GraphExecutionError::MissingInput {
                            node: node.id,
                            buffer: *input,
                        })?;
                    let GraphValue::Buffer(buffer) = input_value else {
                        return Err(GraphExecutionError::InputKindMismatch {
                            node: node.id,
                            value: *input,
                        });
                    };
                    let ir = Ir::App {
                        func: Box::new(Ir::Var("NUMERIC-BUFFER-MAP".into())),
                        args: vec![function.clone(), Ir::Buffer(buffer.clone())],
                    };
                    GraphValue::Buffer(self.execute_map(node, &ir)?)
                }
                ExecutionOperation::FpgaProgram { job } => {
                    let ExecutionTarget::Fpga { device } = &node.target else {
                        return Err(GraphExecutionError::Backend {
                            node: node.id,
                            target: node.target.clone(),
                            message: "FpgaProgram requires an FPGA target".into(),
                        });
                    };
                    let executor = self.fpga.get(device).ok_or_else(|| {
                        GraphExecutionError::TargetUnavailable {
                            node: node.id,
                            target: node.target.clone(),
                        }
                    })?;
                    GraphValue::LispWord(executor.execute_program(job).map_err(|message| {
                        GraphExecutionError::Backend {
                            node: node.id,
                            target: node.target.clone(),
                            message,
                        }
                    })?)
                }
            };

            values.insert(node.output, output);
            completed.insert(node.id);
            execution_order.push(node.id);
        }

        Ok(ExecutionResult {
            values,
            execution_order,
        })
    }

    fn execute_map(&self, node: &PlanNode, ir: &Ir) -> Result<BufferLiteral, GraphExecutionError> {
        match &node.target {
            ExecutionTarget::Cpu => {
                CpuComputeBackend
                    .execute(ir)
                    .map_err(|source| GraphExecutionError::Compute {
                        node: node.id,
                        source,
                    })
            }
            ExecutionTarget::Gpu { backend } => {
                let executor = self.gpu.get(backend).ok_or_else(|| {
                    GraphExecutionError::TargetUnavailable {
                        node: node.id,
                        target: node.target.clone(),
                    }
                })?;
                executor
                    .execute_map(ir)
                    .map_err(|message| GraphExecutionError::Backend {
                        node: node.id,
                        target: node.target.clone(),
                        message,
                    })
            }
            ExecutionTarget::Fpga { .. } => Err(GraphExecutionError::Backend {
                node: node.id,
                target: node.target.clone(),
                message: "NumericBufferMap is not supported by the FPGA executor".into(),
            }),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CpuGraphExecutor;

impl CpuGraphExecutor {
    pub fn execute(&self, graph: &ExecutionGraph) -> Result<ExecutionResult, GraphExecutionError> {
        HeterogeneousGraphExecutor::default().execute(graph)
    }
}

pub struct FpgaTransportNodeExecutor<T> {
    executor: RefCell<FpgaJobExecutor<T>>,
}

impl<T> FpgaTransportNodeExecutor<T>
where
    T: FpgaTransport,
{
    pub fn new(transport: T) -> Self {
        Self {
            executor: RefCell::new(FpgaJobExecutor::new(transport)),
        }
    }
}

impl<T> FpgaProgramNodeExecutor for FpgaTransportNodeExecutor<T>
where
    T: FpgaTransport,
{
    fn execute_program(&self, job: &FpgaJobV1) -> Result<u32, String> {
        self.executor
            .borrow_mut()
            .execute_word(job)
            .map_err(|error| format!("{error:?}"))
    }
}

#[cfg(feature = "gpu-cuda")]
#[derive(Debug, Clone, Copy)]
pub struct CudaNodeExecutor {
    pub device_ordinal: usize,
}

#[cfg(feature = "gpu-cuda")]
impl NodeExecutor for CudaNodeExecutor {
    fn execute_map(&self, ir: &Ir) -> Result<BufferLiteral, String> {
        crate::gpu_cuda_runtime::execute_map(ir, self.device_ordinal)
            .map(|execution| execution.output)
            .map_err(|error| format!("{error:?}"))
    }
}

#[cfg(feature = "gpu-wgpu")]
#[derive(Debug, Clone, Copy)]
pub struct WgpuNodeExecutor {
    pub policy: crate::gpu_wgpu_runtime::AdapterPolicy,
}

#[cfg(feature = "gpu-wgpu")]
impl NodeExecutor for WgpuNodeExecutor {
    fn execute_map(&self, ir: &Ir) -> Result<BufferLiteral, String> {
        crate::gpu_wgpu_runtime::execute_map_blocking_with_policy(ir, self.policy)
            .map(|execution| execution.output)
            .map_err(|error| format!("{error:?}"))
    }
}

fn validate_graph(graph: &ExecutionGraph) -> Result<(), GraphExecutionError> {
    let mut buffers = HashSet::new();
    for (id, _) in &graph.inputs {
        if !buffers.insert(*id) {
            return Err(GraphExecutionError::DuplicateInput(*id));
        }
    }

    let mut nodes = HashSet::new();
    for node in &graph.nodes {
        if !nodes.insert(node.id) {
            return Err(GraphExecutionError::DuplicateNode(node.id));
        }
    }

    for node in &graph.nodes {
        for dependency in &node.dependencies {
            if !nodes.contains(dependency) {
                return Err(GraphExecutionError::MissingDependency {
                    node: node.id,
                    dependency: *dependency,
                });
            }
        }
        if !buffers.insert(node.output) {
            return Err(GraphExecutionError::DuplicateOutput(node.output));
        }
    }
    Ok(())
}
