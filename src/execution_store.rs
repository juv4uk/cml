//! Explicit graph value storage boundary.
//!
//! The scheduler owns ordering; this store owns values published by graph
//! inputs and completed nodes. It intentionally exposes no device policy.

use std::collections::HashMap;

use crate::execution::{BufferId, GraphValue, NodeId};

#[derive(Debug, Default)]
pub(crate) struct GraphValueStore {
    values: HashMap<BufferId, GraphValue>,
}

impl GraphValueStore {
    pub(crate) fn from_inputs(inputs: &[(BufferId, GraphValue)]) -> Self {
        Self {
            values: inputs.iter().cloned().collect(),
        }
    }

    pub(crate) fn get(&self, id: &BufferId) -> Option<&GraphValue> {
        self.values.get(id)
    }

    pub(crate) fn publish(&mut self, id: BufferId, value: GraphValue) {
        self.values.insert(id, value);
    }

    pub(crate) fn into_result(self, execution_order: Vec<NodeId>) -> super::execution::ExecutionResult {
        super::execution::ExecutionResult::from_store(self.values, execution_order)
    }
}
