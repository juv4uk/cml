//! Explicit dependency scheduler for heterogeneous execution graphs.

use std::collections::{HashMap, HashSet};

use crate::execution::{
    ConcurrencyModel, ConcurrencyProfile, ExecutionGraph, ExecutionTarget, GraphExecutionError,
    NodeId, PlanNode,
};

#[derive(Debug, Default, Clone)]
pub struct GraphScheduler {
    completed: HashSet<NodeId>,
    execution_order: Vec<NodeId>,
}

impl GraphScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_ready<'a>(&self, graph: &'a ExecutionGraph) -> Option<&'a PlanNode> {
        graph.nodes.iter().find(|node| {
            !self.completed.contains(&node.id)
                && node
                    .dependencies
                    .iter()
                    .all(|dependency| self.completed.contains(dependency))
        })
    }

    /// Return all nodes whose data dependencies are satisfied and which have not yet completed.
    pub fn ready_set<'a>(&self, graph: &'a ExecutionGraph) -> Vec<&'a PlanNode> {
        graph
            .nodes
            .iter()
            .filter(|node| {
                !self.completed.contains(&node.id)
                    && node
                        .dependencies
                        .iter()
                        .all(|dependency| self.completed.contains(dependency))
            })
            .collect()
    }

    /// Given the currently ready nodes and a function to resolve the concurrency profile
    /// for a target, select a maximal subset of nodes that can be executed concurrently
    /// without violating any physical resource inflight limits.
    /// If an executor or profile is missing, defaults fail-closed to Serial (max 1 inflight).
    pub fn schedule_concurrent_batch<'a, F>(
        &self,
        ready: &[&'a PlanNode],
        get_profile: F,
    ) -> Vec<&'a PlanNode>
    where
        F: FnMut(&ExecutionTarget) -> Option<ConcurrencyProfile>,
    {
        self.schedule_concurrent_batch_bounded(ready, None, get_profile)
    }

    /// Select a concurrent batch of ready nodes respecting physical resource limits and
    /// an optional maximum batch size limit (e.g. bounded worker pool).
    pub fn schedule_concurrent_batch_bounded<'a, F>(
        &self,
        ready: &[&'a PlanNode],
        max_batch: Option<usize>,
        mut get_profile: F,
    ) -> Vec<&'a PlanNode>
    where
        F: FnMut(&ExecutionTarget) -> Option<ConcurrencyProfile>,
    {
        let mut batch = Vec::new();
        let mut inflight_by_resource: HashMap<String, usize> = HashMap::new();

        for &node in ready {
            if let Some(limit) = max_batch {
                if batch.len() >= limit {
                    break;
                }
            }
            let profile = get_profile(&node.target).unwrap_or_else(|| {
                // Fail-closed default: serialize all unknown/unregistered targets under a shared serial resource
                match &node.target {
                    ExecutionTarget::Cpu => ConcurrencyProfile::serial("unregistered-cpu"),
                    ExecutionTarget::Gpu { backend } => {
                        ConcurrencyProfile::serial(format!("unregistered-gpu:{backend}"))
                    }
                    ExecutionTarget::Fpga { device } => {
                        ConcurrencyProfile::serial(format!("unregistered-fpga:{device}"))
                    }
                }
            });

            match profile.model {
                ConcurrencyModel::Serial => {
                    let current = inflight_by_resource
                        .entry(profile.resource_key.clone())
                        .or_insert(0);
                    if *current == 0 {
                        *current += 1;
                        batch.push(node);
                    }
                }
                ConcurrencyModel::Bounded(max) => {
                    let current = inflight_by_resource
                        .entry(profile.resource_key.clone())
                        .or_insert(0);
                    if *current < max {
                        *current += 1;
                        batch.push(node);
                    }
                }
                ConcurrencyModel::Reentrant => {
                    let current = inflight_by_resource
                        .entry(profile.resource_key.clone())
                        .or_insert(0);
                    *current += 1;
                    batch.push(node);
                }
            }
        }

        batch
    }

    pub fn complete(&mut self, node: NodeId) {
        self.completed.insert(node);
        self.execution_order.push(node);
    }

    pub fn is_finished(&self, graph: &ExecutionGraph) -> bool {
        self.completed.len() == graph.nodes.len()
    }

    pub fn execution_order(&self) -> &[NodeId] {
        &self.execution_order
    }

    pub fn finish(self) -> Vec<NodeId> {
        self.execution_order
    }

    pub fn cycle_error(&self) -> GraphExecutionError {
        GraphExecutionError::DependencyCycle
    }
}
