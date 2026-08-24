//! Explicit dependency scheduler for heterogeneous execution graphs.

use std::collections::HashSet;

use crate::execution::{ExecutionGraph, GraphExecutionError, NodeId, PlanNode};

#[derive(Debug, Default)]
pub(crate) struct GraphScheduler {
    completed: HashSet<NodeId>,
    execution_order: Vec<NodeId>,
}

impl GraphScheduler {
    pub(crate) fn next_ready<'a>(&self, graph: &'a ExecutionGraph) -> Option<&'a PlanNode> {
        graph.nodes.iter().find(|node| {
            !self.completed.contains(&node.id)
                && node
                    .dependencies
                    .iter()
                    .all(|dependency| self.completed.contains(dependency))
        })
    }

    pub(crate) fn complete(&mut self, node: NodeId) {
        self.completed.insert(node);
        self.execution_order.push(node);
    }

    pub(crate) fn is_finished(&self, graph: &ExecutionGraph) -> bool {
        self.completed.len() == graph.nodes.len()
    }

    pub(crate) fn finish(self) -> Vec<NodeId> {
        self.execution_order
    }

    pub(crate) fn cycle_error(&self) -> GraphExecutionError {
        GraphExecutionError::DependencyCycle
    }
}
