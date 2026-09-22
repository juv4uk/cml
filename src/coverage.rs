//! Machine-readable CML coverage foundation (#106).
//!
//! Language identity comes from the supported-pin my-lisp semantic registry,
//! generated into `canon` by build.rs. This module only joins those opaque
//! IDs with CML's existing operation-admission table. It intentionally does
//! not infer backend executability from source admission.

use crate::canon::{
    CANON_SUPPORTED_PIN_REGISTRY_FNV1A64, CANON_SUPPORTED_PIN_SEMANTIC_IDS, find_operation_by_id,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionState {
    SourceAdmitted,
    NotYetAdmitted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCoverageRow {
    pub semantic_id: &'static str,
    pub admission: AdmissionState,
    pub operation_status: Option<&'static str>,
    pub evidence: Option<&'static str>,
    /// Reserved for evidence-backed target slices. The foundation must not
    /// upgrade a source-admission fact into an execution claim.
    pub backend_evidence: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageSummary {
    pub semantic_identities: usize,
    pub source_admitted: usize,
    pub not_yet_admitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageLedger {
    pub upstream_channel: &'static str,
    pub registry_digest_fnv1a64: u64,
    pub rows: Vec<SemanticCoverageRow>,
}

impl CoverageLedger {
    pub fn supported_pin() -> Self {
        let rows = CANON_SUPPORTED_PIN_SEMANTIC_IDS
            .iter()
            .map(|semantic_id| {
                let semantic_id = *semantic_id;
                if let Some(operation) = find_operation_by_id(semantic_id) {
                    SemanticCoverageRow {
                        semantic_id,
                        admission: AdmissionState::SourceAdmitted,
                        operation_status: Some(operation.status),
                        evidence: Some(operation.provenance_witness),
                        backend_evidence: None,
                    }
                } else {
                    SemanticCoverageRow {
                        semantic_id,
                        admission: AdmissionState::NotYetAdmitted,
                        operation_status: None,
                        evidence: None,
                        backend_evidence: None,
                    }
                }
            })
            .collect();

        Self {
            upstream_channel: "supported-pin",
            registry_digest_fnv1a64: CANON_SUPPORTED_PIN_REGISTRY_FNV1A64,
            rows,
        }
    }

    pub fn row(&self, semantic_id: &str) -> Option<&SemanticCoverageRow> {
        self.rows.iter().find(|row| row.semantic_id == semantic_id)
    }

    pub fn summary(&self) -> CoverageSummary {
        let source_admitted = self
            .rows
            .iter()
            .filter(|row| row.admission == AdmissionState::SourceAdmitted)
            .count();
        let not_yet_admitted = self.rows.len() - source_admitted;

        CoverageSummary {
            semantic_identities: self.rows.len(),
            source_admitted,
            not_yet_admitted,
        }
    }
}
