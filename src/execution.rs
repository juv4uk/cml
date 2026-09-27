//! Device-neutral execution graph and CPU reference executor.
//!
//! M0 deliberately executes only CPU nodes. GPU and FPGA targets are
//! representable so the graph contract does not need to change later, but
//! they fail closed until a live executor is registered.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::compute::{ComputeBackend, ComputeExecutionError, CpuComputeBackend};
pub use crate::execution_scheduler::GraphScheduler;
use crate::execution_store::GraphValueStore;
use crate::fpga_transport::{
    FpgaJobExecutor, FpgaJobV1, FpgaTransport, encode_i32_buffer_as_register_inputs,
};
use crate::ir::{BufferLiteral, Ir};

/// Execution mode controlling physical concurrency of ready nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionMode {
    /// Execute independent ready nodes concurrently up to executor concurrency limits.
    #[default]
    Concurrent,
    /// Execute nodes strictly one at a time in deterministic topological ready order.
    Sequential,
}

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
    NumericBufferMap {
        function: Ir,
        input: BufferId,
    },
    FpgaProgram {
        job: FpgaJobV1,
    },
    FpgaProgramWithBufferInput {
        job: FpgaJobV1,
        input: BufferId,
        first_register: u8,
    },
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
    pub(crate) fn from_store(
        values: HashMap<BufferId, GraphValue>,
        execution_order: Vec<NodeId>,
    ) -> Self {
        Self {
            values,
            execution_order,
        }
    }

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

    fn retain_requested(mut self, requested: &HashSet<BufferId>) -> Self {
        self.values.retain(|id, _| requested.contains(id));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphExecutionError {
    DuplicateInput(BufferId),
    DuplicateNode(NodeId),
    DuplicateOutput(BufferId),
    UnknownRequestedBuffer(BufferId),
    MissingDependency {
        node: NodeId,
        dependency: NodeId,
    },
    DependencyCycle,
    MissingInput {
        node: NodeId,
        buffer: BufferId,
    },
    MissingValueProducer {
        node: NodeId,
        value: BufferId,
    },
    MissingDataDependency {
        node: NodeId,
        value: BufferId,
        producer: NodeId,
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

/// Explicit concurrency model declared by a backend mechanism.
/// Concurrency is a property of mechanism/provenance and physical device constraints,
/// never inferred from language semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConcurrencyModel {
    /// Strict single-owner serialization (e.g. physical serial/COM transport).
    Serial,
    /// Bounded concurrency up to `max_inflight` parallel executions.
    Bounded(usize),
    /// Reentrant / unconstrained concurrency.
    Reentrant,
}

/// A machine-readable concurrency profile describing an executor's physical concurrency
/// properties and physical resource ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcurrencyProfile {
    pub model: ConcurrencyModel,
    pub max_inflight: usize,
    /// Identifier for the physical device/resource (e.g. "cpu", "cuda:0", "fpga:com4").
    /// Backends sharing the same physical resource key share inflight accounting.
    pub resource_key: String,
}

impl ConcurrencyProfile {
    pub fn serial(resource_key: impl Into<String>) -> Self {
        Self {
            model: ConcurrencyModel::Serial,
            max_inflight: 1,
            resource_key: resource_key.into(),
        }
    }

    pub fn bounded(max_inflight: usize, resource_key: impl Into<String>) -> Self {
        let max_inflight = max_inflight.max(1);
        Self {
            model: ConcurrencyModel::Bounded(max_inflight),
            max_inflight,
            resource_key: resource_key.into(),
        }
    }

    pub fn reentrant(resource_key: impl Into<String>) -> Self {
        Self {
            model: ConcurrencyModel::Reentrant,
            max_inflight: usize::MAX,
            resource_key: resource_key.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeChainError {
    pub step: usize,
    pub message: String,
}

pub trait NodeExecutor: Send + Sync {
    fn execute_map(&self, ir: &Ir) -> Result<BufferLiteral, String>;

    /// Optional mechanism-only fast path for a linear map chain. Returning
    /// None preserves the ordinary node-by-node graph executor.
    fn execute_map_chain(
        &self,
        _functions: &[Ir],
        _input: &BufferLiteral,
    ) -> Result<Option<Vec<BufferLiteral>>, NodeChainError> {
        Ok(None)
    }

    /// Optional selective-materialization fast path. The default preserves
    /// correctness by executing the ordinary chain and filtering host outputs.
    fn execute_map_chain_selected(
        &self,
        functions: &[Ir],
        input: &BufferLiteral,
        requested_steps: &[usize],
    ) -> Result<Option<Vec<(usize, BufferLiteral)>>, NodeChainError> {
        let Some(outputs) = self.execute_map_chain(functions, input)? else {
            return Ok(None);
        };
        let mut selected = Vec::with_capacity(requested_steps.len());
        for &step in requested_steps {
            let Some(output) = outputs.get(step) else {
                return Err(NodeChainError {
                    step,
                    message: format!(
                        "requested chain step {step} is outside {} outputs",
                        outputs.len()
                    ),
                });
            };
            selected.push((step, output.clone()));
        }
        Ok(Some(selected))
    }

    /// Mechanism concurrency profile. Defaults to single-inflight Serial if not overridden.
    fn concurrency_profile(&self) -> ConcurrencyProfile {
        ConcurrencyProfile::serial("unspecified-node-executor")
    }
}

pub trait FpgaProgramNodeExecutor: Send + Sync {
    fn execute_program(&self, job: &FpgaJobV1) -> Result<u32, String>;

    /// Mechanism concurrency profile. Defaults to single-inflight Serial for hardware safety.
    fn concurrency_profile(&self) -> ConcurrencyProfile {
        ConcurrencyProfile::serial("unspecified-fpga-executor")
    }
}

#[derive(Default)]
pub struct HeterogeneousGraphExecutor {
    gpu: HashMap<String, Box<dyn NodeExecutor>>,
    fpga: HashMap<String, Box<dyn FpgaProgramNodeExecutor>>,
    max_workers: Option<usize>,
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

    pub fn concurrency_profile(&self, target: &ExecutionTarget) -> Option<ConcurrencyProfile> {
        match target {
            ExecutionTarget::Cpu => Some(ConcurrencyProfile::reentrant("cpu")),
            ExecutionTarget::Gpu { backend } => {
                self.gpu.get(backend).map(|e| e.concurrency_profile())
            }
            ExecutionTarget::Fpga { device } => {
                self.fpga.get(device).map(|e| e.concurrency_profile())
            }
        }
    }

    /// Return all nodes currently ready to execute in the given graph state.
    pub fn ready_set<'a>(
        &self,
        graph: &'a ExecutionGraph,
        completed: &std::collections::HashSet<NodeId>,
    ) -> Vec<&'a PlanNode> {
        let mut scheduler = GraphScheduler::default();
        for id in completed {
            scheduler.complete(*id);
        }
        scheduler.ready_set(graph)
    }

    pub fn with_max_workers(mut self, max_workers: usize) -> Self {
        self.max_workers = Some(max_workers);
        self
    }

    pub fn set_max_workers(&mut self, max_workers: usize) {
        self.max_workers = Some(max_workers);
    }

    pub fn max_workers(&self) -> Option<usize> {
        self.max_workers
    }

    /// Partition a list of ready nodes into concurrent batches where each batch
    /// respects the concurrency profile and max_inflight limit of each physical resource.
    pub fn schedule_batches<'a>(&self, ready: &[&'a PlanNode]) -> Vec<Vec<&'a PlanNode>> {
        let scheduler = GraphScheduler::default();
        let mut remaining: Vec<&'a PlanNode> = ready.to_vec();
        let mut batches = Vec::new();

        while !remaining.is_empty() {
            let batch = scheduler.schedule_concurrent_batch_bounded(
                &remaining,
                self.max_workers,
                |target| self.concurrency_profile(target),
            );
            if batch.is_empty() {
                // Fallback to avoid infinite loop if no node could be scheduled
                batches.push(vec![remaining.remove(0)]);
            } else {
                let batch_ids: std::collections::HashSet<NodeId> =
                    batch.iter().map(|n| n.id).collect();
                remaining.retain(|n| !batch_ids.contains(&n.id));
                batches.push(batch);
            }
        }

        batches
    }

    pub fn execute(&self, graph: &ExecutionGraph) -> Result<ExecutionResult, GraphExecutionError> {
        self.execute_mode(graph, ExecutionMode::Concurrent)
    }

    /// Execute the complete graph but expose only explicitly requested host
    /// values. Backends may use this policy to avoid unnecessary readback.
    ///
    /// The ordinary execute() contract remains unchanged and materializes all
    /// graph outputs.
    pub fn execute_requested(
        &self,
        graph: &ExecutionGraph,
        requested: &[BufferId],
    ) -> Result<ExecutionResult, GraphExecutionError> {
        validate_graph(graph)?;
        let available: HashSet<BufferId> = graph
            .inputs
            .iter()
            .map(|(id, _)| *id)
            .chain(graph.nodes.iter().map(|node| node.output))
            .collect();
        for &id in requested {
            if !available.contains(&id) {
                return Err(GraphExecutionError::UnknownRequestedBuffer(id));
            }
        }
        let requested: HashSet<BufferId> = requested.iter().copied().collect();

        if let Some(result) =
            self.try_execute_linear_gpu_map_chain_requested(graph, Some(&requested))?
        {
            return Ok(result);
        }

        Ok(self
            .execute_mode(graph, ExecutionMode::Concurrent)?
            .retain_requested(&requested))
    }

    pub fn execute_sequential(
        &self,
        graph: &ExecutionGraph,
    ) -> Result<ExecutionResult, GraphExecutionError> {
        self.execute_mode(graph, ExecutionMode::Sequential)
    }

    pub fn execute_mode(
        &self,
        graph: &ExecutionGraph,
        mode: ExecutionMode,
    ) -> Result<ExecutionResult, GraphExecutionError> {
        validate_graph(graph)?;
        if let Some(result) = self.try_execute_linear_gpu_map_chain(graph)? {
            return Ok(result);
        }
        let mut values = GraphValueStore::from_inputs(&graph.inputs);
        let mut scheduler = GraphScheduler::default();

        while !scheduler.is_finished(graph) {
            match mode {
                ExecutionMode::Sequential => {
                    let Some(node) = scheduler.next_ready(graph) else {
                        return Err(scheduler.cycle_error());
                    };
                    let output = self.execute_node(node, &values)?;
                    values.publish(node.output, output);
                    scheduler.complete(node.id);
                }
                ExecutionMode::Concurrent => {
                    let ready = scheduler.ready_set(graph);
                    if ready.is_empty() {
                        return Err(scheduler.cycle_error());
                    }

                    let batch = scheduler.schedule_concurrent_batch_bounded(
                        &ready,
                        self.max_workers,
                        |target| self.concurrency_profile(target),
                    );
                    let batch = if batch.is_empty() {
                        vec![ready[0]]
                    } else {
                        batch
                    };

                    if batch.len() == 1 {
                        let node = batch[0];
                        let output = self.execute_node(node, &values)?;
                        values.publish(node.output, output);
                        scheduler.complete(node.id);
                    } else {
                        let values_ref = &values;
                        let results: Vec<
                            Result<(NodeId, BufferId, GraphValue), GraphExecutionError>,
                        > = std::thread::scope(|s| {
                            let mut handles = Vec::with_capacity(batch.len());
                            for &node in &batch {
                                let handle = s.spawn(move || {
                                    let output = self.execute_node(node, values_ref)?;
                                    Ok((node.id, node.output, output))
                                });
                                handles.push(handle);
                            }
                            handles
                                .into_iter()
                                .map(|h| h.join().expect("graph worker thread panicked"))
                                .collect()
                        });

                        // Named deterministic failure policy:
                        // Scan results in deterministic batch order. The first failure in batch order is returned.
                        for res in &results {
                            if let Err(err) = res {
                                return Err(err.clone());
                            }
                        }

                        // Atomic value publication and completion in deterministic batch order.
                        for res in results {
                            let (node_id, output_id, output_val) = res.unwrap();
                            values.publish(output_id, output_val);
                            scheduler.complete(node_id);
                        }
                    }
                }
            }
        }

        Ok(values.into_result(scheduler.finish()))
    }

    fn try_execute_linear_gpu_map_chain(
        &self,
        graph: &ExecutionGraph,
    ) -> Result<Option<ExecutionResult>, GraphExecutionError> {
        self.try_execute_linear_gpu_map_chain_requested(graph, None)
    }

    fn try_execute_linear_gpu_map_chain_requested(
        &self,
        graph: &ExecutionGraph,
        requested: Option<&HashSet<BufferId>>,
    ) -> Result<Option<ExecutionResult>, GraphExecutionError> {
        if graph.nodes.len() < 2 {
            return Ok(None);
        }

        let first = &graph.nodes[0];
        let ExecutionTarget::Gpu { backend } = &first.target else {
            return Ok(None);
        };
        if !first.dependencies.is_empty() {
            return Ok(None);
        }
        let ExecutionOperation::NumericBufferMap {
            function: first_function,
            input: first_input,
        } = &first.operation
        else {
            return Ok(None);
        };

        let input_value = graph
            .inputs
            .iter()
            .find_map(|(id, value)| (id == first_input).then_some(value))
            .ok_or(GraphExecutionError::MissingInput {
                node: first.id,
                buffer: *first_input,
            })?;
        let GraphValue::Buffer(input_buffer) = input_value else {
            return Err(GraphExecutionError::InputKindMismatch {
                node: first.id,
                value: *first_input,
            });
        };

        let mut functions = Vec::with_capacity(graph.nodes.len());
        functions.push(first_function.clone());
        for index in 1..graph.nodes.len() {
            let previous = &graph.nodes[index - 1];
            let node = &graph.nodes[index];
            if node.target != first.target || node.dependencies.as_slice() != [previous.id] {
                return Ok(None);
            }
            let ExecutionOperation::NumericBufferMap { function, input } = &node.operation else {
                return Ok(None);
            };
            if *input != previous.output {
                return Ok(None);
            }
            functions.push(function.clone());
        }

        let requested_steps: Vec<usize> = match requested {
            Some(requested) => graph
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(step, node)| requested.contains(&node.output).then_some(step))
                .collect(),
            None => (0..graph.nodes.len()).collect(),
        };

        let executor =
            self.gpu
                .get(backend)
                .ok_or_else(|| GraphExecutionError::TargetUnavailable {
                    node: first.id,
                    target: first.target.clone(),
                })?;
        let outputs =
            match executor.execute_map_chain_selected(&functions, input_buffer, &requested_steps) {
                Ok(Some(outputs)) => outputs,
                Ok(None) => return Ok(None),
                Err(error) => {
                    let node = graph.nodes.get(error.step).unwrap_or(first);
                    return Err(GraphExecutionError::Backend {
                        node: node.id,
                        target: node.target.clone(),
                        message: error.message,
                    });
                }
            };
        if outputs.len() != requested_steps.len() {
            return Err(GraphExecutionError::Backend {
                node: first.id,
                target: first.target.clone(),
                message: format!(
                    "selected map-chain executor returned {} outputs for {} requested steps",
                    outputs.len(),
                    requested_steps.len()
                ),
            });
        }

        let mut values: HashMap<BufferId, GraphValue> = match requested {
            Some(requested) => graph
                .inputs
                .iter()
                .filter(|(id, _)| requested.contains(id))
                .cloned()
                .collect(),
            None => graph.inputs.iter().cloned().collect(),
        };
        for (expected_step, (step, output)) in requested_steps.iter().copied().zip(outputs) {
            if step != expected_step {
                return Err(GraphExecutionError::Backend {
                    node: first.id,
                    target: first.target.clone(),
                    message: format!(
                        "selected map-chain executor returned step {step}, expected {expected_step}"
                    ),
                });
            }
            let node = &graph.nodes[step];
            values.insert(node.output, GraphValue::Buffer(output));
        }

        let execution_order = graph.nodes.iter().map(|node| node.id).collect();
        Ok(Some(ExecutionResult::from_store(values, execution_order)))
    }

    fn execute_node(
        &self,
        node: &PlanNode,
        values: &GraphValueStore,
    ) -> Result<GraphValue, GraphExecutionError> {
        match &node.operation {
            ExecutionOperation::NumericBufferMap { function, input } => {
                let input_value = values.get(input).ok_or(GraphExecutionError::MissingInput {
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
                    func: Box::new(Ir::Sid(sens::sens!(01011001))),
                    args: vec![function.clone(), Ir::Buffer(buffer.clone())],
                };
                Ok(GraphValue::Buffer(self.execute_map(node, &ir)?))
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
                Ok(GraphValue::LispWord(
                    executor.execute_program(job).map_err(|message| {
                        GraphExecutionError::Backend {
                            node: node.id,
                            target: node.target.clone(),
                            message,
                        }
                    })?,
                ))
            }
            ExecutionOperation::FpgaProgramWithBufferInput {
                job,
                input,
                first_register,
            } => {
                let ExecutionTarget::Fpga { device } = &node.target else {
                    return Err(GraphExecutionError::Backend {
                        node: node.id,
                        target: node.target.clone(),
                        message: "FpgaProgramWithBufferInput requires an FPGA target".into(),
                    });
                };
                let input_value = values.get(input).ok_or(GraphExecutionError::MissingInput {
                    node: node.id,
                    buffer: *input,
                })?;
                let GraphValue::Buffer(buffer) = input_value else {
                    return Err(GraphExecutionError::InputKindMismatch {
                        node: node.id,
                        value: *input,
                    });
                };
                if !job.register_inputs.is_empty() {
                    return Err(GraphExecutionError::Backend {
                        node: node.id,
                        target: node.target.clone(),
                        message: "typed FPGA input job must not prepopulate register inputs".into(),
                    });
                }
                let mut staged_job = job.clone();
                staged_job.register_inputs =
                    encode_i32_buffer_as_register_inputs(buffer, *first_register).map_err(
                        |error| GraphExecutionError::Backend {
                            node: node.id,
                            target: node.target.clone(),
                            message: format!("typed FPGA input rejected: {error:?}"),
                        },
                    )?;
                let executor = self.fpga.get(device).ok_or_else(|| {
                    GraphExecutionError::TargetUnavailable {
                        node: node.id,
                        target: node.target.clone(),
                    }
                })?;
                Ok(GraphValue::LispWord(
                    executor.execute_program(&staged_job).map_err(|message| {
                        GraphExecutionError::Backend {
                            node: node.id,
                            target: node.target.clone(),
                            message,
                        }
                    })?,
                ))
            }
        }
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

    pub fn execute_sequential(
        &self,
        graph: &ExecutionGraph,
    ) -> Result<ExecutionResult, GraphExecutionError> {
        HeterogeneousGraphExecutor::default().execute_sequential(graph)
    }

    pub fn execute_mode(
        &self,
        graph: &ExecutionGraph,
        mode: ExecutionMode,
    ) -> Result<ExecutionResult, GraphExecutionError> {
        HeterogeneousGraphExecutor::default().execute_mode(graph, mode)
    }
}

/// Node executor backed by ParallelCpuComputeBackend with explicit worker count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParallelCpuNodeExecutor {
    backend: crate::compute::ParallelCpuComputeBackend,
}

impl ParallelCpuNodeExecutor {
    pub fn new(workers: usize) -> Self {
        Self {
            backend: crate::compute::ParallelCpuComputeBackend::new(workers),
        }
    }

    pub fn workers(&self) -> usize {
        self.backend.workers()
    }
}

impl Default for ParallelCpuNodeExecutor {
    fn default() -> Self {
        Self {
            backend: crate::compute::ParallelCpuComputeBackend::default(),
        }
    }
}

impl NodeExecutor for ParallelCpuNodeExecutor {
    fn execute_map(&self, ir: &Ir) -> Result<BufferLiteral, String> {
        self.backend
            .execute(ir)
            .map_err(|error| format!("{error:?}"))
    }

    fn concurrency_profile(&self) -> ConcurrencyProfile {
        ConcurrencyProfile::bounded(self.backend.workers(), "cpu-multicore")
    }
}

pub struct FpgaTransportNodeExecutor<T> {
    executor: Mutex<FpgaJobExecutor<T>>,
}

impl<T> FpgaTransportNodeExecutor<T>
where
    T: FpgaTransport,
{
    pub fn new(transport: T) -> Self {
        Self {
            executor: Mutex::new(FpgaJobExecutor::new(transport)),
        }
    }
}

impl<T> FpgaProgramNodeExecutor for FpgaTransportNodeExecutor<T>
where
    T: FpgaTransport,
{
    fn execute_program(&self, job: &FpgaJobV1) -> Result<u32, String> {
        self.executor
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?
            .execute_word(job)
            .map_err(|error| format!("{error:?}"))
    }

    fn concurrency_profile(&self) -> ConcurrencyProfile {
        ConcurrencyProfile::serial("fpga-transport")
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

    fn execute_map_chain(
        &self,
        functions: &[Ir],
        input: &BufferLiteral,
    ) -> Result<Option<Vec<BufferLiteral>>, NodeChainError> {
        if !matches!(input, BufferLiteral::I32(_)) {
            return Ok(None);
        }
        crate::gpu_cuda_runtime::execute_map_chain_i32(functions, input, self.device_ordinal)
            .map(|execution| Some(execution.outputs))
            .map_err(|error| NodeChainError {
                step: error.step,
                message: format!(
                    "CUDA resident map chain failed at step {}: {:?}",
                    error.step, error.source
                ),
            })
    }

    fn execute_map_chain_selected(
        &self,
        functions: &[Ir],
        input: &BufferLiteral,
        requested_steps: &[usize],
    ) -> Result<Option<Vec<(usize, BufferLiteral)>>, NodeChainError> {
        if !matches!(input, BufferLiteral::I32(_)) {
            return Ok(None);
        }
        crate::gpu_cuda_runtime::execute_map_chain_i32_selected(
            functions,
            input,
            requested_steps,
            self.device_ordinal,
        )
        .map(|execution| Some(execution.outputs))
        .map_err(|error| NodeChainError {
            step: error.step,
            message: format!(
                "CUDA selective resident map chain failed at step {}: {:?}",
                error.step, error.source
            ),
        })
    }

    fn concurrency_profile(&self) -> ConcurrencyProfile {
        ConcurrencyProfile::bounded(1, format!("cuda:{}", self.device_ordinal))
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

    fn concurrency_profile(&self) -> ConcurrencyProfile {
        ConcurrencyProfile::bounded(1, "wgpu-device")
    }
}

fn validate_graph(graph: &ExecutionGraph) -> Result<(), GraphExecutionError> {
    let mut buffers = HashSet::new();
    let input_buffers: HashSet<_> = graph.inputs.iter().map(|(id, _)| *id).collect();
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

    let mut producers = HashMap::new();
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
        producers.insert(node.output, node.id);
    }

    for node in &graph.nodes {
        let input = match &node.operation {
            ExecutionOperation::NumericBufferMap { input, .. }
            | ExecutionOperation::FpgaProgramWithBufferInput { input, .. } => input,
            ExecutionOperation::FpgaProgram { .. } => continue,
        };
        if input_buffers.contains(input) {
            continue;
        }
        let producer =
            producers
                .get(input)
                .copied()
                .ok_or(GraphExecutionError::MissingValueProducer {
                    node: node.id,
                    value: *input,
                })?;
        if !node.dependencies.contains(&producer) {
            return Err(GraphExecutionError::MissingDataDependency {
                node: node.id,
                value: *input,
                producer,
            });
        }
    }
    Ok(())
}
