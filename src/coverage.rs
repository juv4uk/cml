//! Machine-readable CML coverage foundation (#106).
//!
//! Language identity comes from the supported-pin my-lisp semantic registry,
//! generated into `canon` by build.rs. This module only joins those opaque
//! IDs with CML's existing operation-admission table and evidence-backed
//! compiler target facts. It never derives backend execution from source
//! admission alone.

use my_lisp::Sid8;
use crate::canon::{
    CANON_SUPPORTED_PIN_REGISTRY_FNV1A64, CANON_SUPPORTED_PIN_SEMANTIC_IDS, find_operation_by_id,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionState {
    SourceAdmitted,
    NotYetAdmitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendEvidenceState {
    AssemblyWitness,
    Executable,
    ExecutableRepresentationLimited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendEvidence {
    pub backend: &'static str,
    pub state: BackendEvidenceState,
    pub evidence: &'static str,
    pub representation_limit: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCoverageRow {
    pub semantic_id: Sid8,
    pub admission: AdmissionState,
    pub operation_status: Option<&'static str>,
    pub evidence: Option<&'static str>,
    pub backend_evidence: Vec<BackendEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageSummary {
    pub semantic_identities: usize,
    pub source_admitted: usize,
    pub not_yet_admitted: usize,
    pub x86_executable: usize,
    pub x86_assembly_witness_only: usize,
    pub x86_representation_limited: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageLedger {
    pub upstream_channel: &'static str,
    pub registry_digest_fnv1a64: u64,
    pub rows: Vec<SemanticCoverageRow>,
}

fn x86_canon_evidence(semantic_id: Sid8) -> Vec<BackendEvidence> {
    let (state, evidence) = if semantic_id == my_lisp::sid!(00000001) {
        (
            BackendEvidenceState::Executable,
            "x86_freestanding_test.rs: compiler_corpus_quote_radio_returns_the_interned_symbol",
        )
    } else if semantic_id == my_lisp::sid!(00000010) {
        (
            BackendEvidenceState::AssemblyWitness,
            "x86_freestanding_test.rs: primitive_slice_uses_only_ratified_runtime_imports",
        )
    } else if semantic_id == my_lisp::sid!(00000011) {
        (
            BackendEvidenceState::Executable,
            "x86_freestanding_test.rs: compiler_corpus_eq_on_matching_and_differing_quoted_symbols",
        )
    } else if semantic_id == my_lisp::sid!(00000100) {
        (
            BackendEvidenceState::Executable,
            "x86_freestanding_test.rs: frozen_cons_fixture_is_deterministic_and_assembles; named_definition_allocates_a_list_through_the_asm_nucleus",
        )
    } else if semantic_id == my_lisp::sid!(00000101) {
        (
            BackendEvidenceState::AssemblyWitness,
            "x86_freestanding_test.rs: primitive_slice_uses_only_ratified_runtime_imports",
        )
    } else if semantic_id == my_lisp::sid!(00000110) {
        (
            BackendEvidenceState::AssemblyWitness,
            "x86_freestanding_test.rs: primitive_slice_uses_only_ratified_runtime_imports",
        )
    } else if semantic_id == my_lisp::sid!(00000111) {
        (
            BackendEvidenceState::Executable,
            "x86_freestanding_test.rs: standalone_cond_true_false_branch_selection_witness",
        )
    } else {
        return Vec::new();
    };

    vec![BackendEvidence {
        backend: "x86-freestanding",
        state,
        evidence,
        representation_limit: None,
    }]
}

fn x86_fixnum_arithmetic_evidence(semantic_id: Sid8) -> Vec<BackendEvidence> {
    let evidence = if semantic_id == my_lisp::sid!(00001100) {
        "x86_freestanding_test.rs: named_definition_uses_a_lexical_let_binding; checked_add_and_sub_produce_inline_arithmetic; fixnum_range_is_owned_by_the_target_contract"
    } else if semantic_id == my_lisp::sid!(00001101) {
        "x86_freestanding_test.rs: out_of_line_named_self_tail_recursion_reuses_its_native_frame; checked_add_and_sub_produce_inline_arithmetic; fixnum_range_is_owned_by_the_target_contract"
    } else {
        return Vec::new();
    };

    vec![BackendEvidence {
        backend: "x86-freestanding",
        state: BackendEvidenceState::ExecutableRepresentationLimited,
        evidence,
        representation_limit: Some(
            "target-fixnum-only; exact-rational x86 representation remains blocked by #105/#137",
        ),
    }]
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
                        backend_evidence: {
                            let mut evidence = x86_canon_evidence(semantic_id);
                            evidence.extend(x86_fixnum_arithmetic_evidence(semantic_id));
                            evidence
                        },
                    }
                } else {
                    SemanticCoverageRow {
                        semantic_id,
                        admission: AdmissionState::NotYetAdmitted,
                        operation_status: None,
                        evidence: None,
                        backend_evidence: Vec::new(),
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

    pub fn row(&self, semantic_id: Sid8) -> Option<&SemanticCoverageRow> {
        self.rows.iter().find(|row| row.semantic_id == semantic_id)
    }

    pub fn summary(&self) -> CoverageSummary {
        let source_admitted = self
            .rows
            .iter()
            .filter(|row| row.admission == AdmissionState::SourceAdmitted)
            .count();
        let not_yet_admitted = self.rows.len() - source_admitted;
        let x86_executable = self
            .rows
            .iter()
            .flat_map(|row| row.backend_evidence.iter())
            .filter(|evidence| {
                evidence.backend == "x86-freestanding"
                    && evidence.state == BackendEvidenceState::Executable
            })
            .count();
        let x86_assembly_witness_only = self
            .rows
            .iter()
            .flat_map(|row| row.backend_evidence.iter())
            .filter(|evidence| {
                evidence.backend == "x86-freestanding"
                    && evidence.state == BackendEvidenceState::AssemblyWitness
            })
            .count();
        let x86_representation_limited = self
            .rows
            .iter()
            .flat_map(|row| row.backend_evidence.iter())
            .filter(|evidence| {
                evidence.backend == "x86-freestanding"
                    && evidence.state == BackendEvidenceState::ExecutableRepresentationLimited
            })
            .count();

        CoverageSummary {
            semantic_identities: self.rows.len(),
            source_admitted,
            not_yet_admitted,
            x86_executable,
            x86_assembly_witness_only,
            x86_representation_limited,
        }
    }
}
