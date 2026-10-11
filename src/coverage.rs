//! Exact-domain CML compiler coverage, supplied by the executable SENS law.
//!
//! This is NOT historical byte-SID coverage. Compiler admission is proved
//! per D3/D4 *width-qualified* identity against pinned SENS authority.
//! Historic x86 witnesses for old byte keys cannot grant modern execution:
//! target evidence stays empty until a new end-to-end oracle proves parity.

use sens::{Bija3, Bit3, Bit4, CompilerLoweringRole, CoreD4, CoreDomainIdentity};

const SOURCE_CORPUS: &str =
    include_str!("../external/sens/contracts/compiler-nucleus-identity-corpus-v1.tsv");
const LANGUAGE_CONTRACT: &[u8] = include_bytes!("../external/sens/language-contract.lisp");

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

/// A current compiler-language *role*, not a legacy semantic byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCoverageRow {
    pub identity: CoreDomainIdentity,
    pub execution_role: CompilerLoweringRole,
    pub admission: AdmissionState,
    pub operation_status: &'static str,
    pub evidence: &'static str,
    pub authority_sha256: String,
    pub backend_evidence: Vec<BackendEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageSummary {
    /// Current SENS compiler-role subset (7 D3 + 2 D4), not all D1..D9 words.
    pub compiler_role_identities: usize,
    pub source_admitted: usize,
    pub not_yet_admitted: usize,
    pub x86_executable: usize,
    pub x86_assembly_witness_only: usize,
    pub x86_representation_limited: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageLedger {
    pub upstream_channel: &'static str,
    pub authority_sha256: String,
    pub rows: Vec<SemanticCoverageRow>,
}

fn exact_identity(domain: &str, bits: &str) -> Result<CoreDomainIdentity, String> {
    let width = match domain {
        "D3" => 3usize,
        "D4" => 4usize,
        other => return Err(format!("unratified compiler role domain {other}")),
    };
    if bits.len() != width || !bits.bytes().all(|c| c == b'0' || c == b'1') {
        return Err(format!(
            "noncanonical width-qualified compiler identity {domain}:{bits}"
        ));
    }
    let raw = u8::from_str_radix(bits, 2)
        .map_err(|_| "unparseable binary compiler coordinate".to_string())?;
    match domain {
        "D3" => Ok(CoreDomainIdentity::D3(Bija3::from_word(
            Bit3::new(raw).ok_or("invalid D3 coordinate")?,
        ))),
        "D4" => Ok(CoreDomainIdentity::D4(CoreD4::from_word(
            Bit4::new(raw).ok_or("invalid D4 coordinate")?,
        ))),
        _ => unreachable!("width-validated D3/D4"),
    }
}

impl CoverageLedger {
    /// Reconstruct coverage only from pinned SENS-owned coordinates and a
    /// runtime-evaluated exact language-law projection. Never use surface
    /// spellings, a historical 8-bit registry, or x86 legacy evidence.
    pub fn try_current_sens() -> Result<Self, String> {
        let authority_sha256 = sens::sha256_source(LANGUAGE_CONTRACT)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let mut rows = Vec::new();
        let mut d3 = 0;
        let mut d4 = 0;
        for source_line in SOURCE_CORPUS.lines() {
            let line = source_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 3 {
                return Err("malformed source-owned compiler corpus".into());
            }
            let (fixture, domain, bits) = (parts[0], parts[1], parts[2]);
            if fixture != format!("nucleus-{}-{bits}", domain.to_ascii_lowercase()) {
                return Err("source-owned compiler fixture and domain disagree".into());
            }
            let identity = exact_identity(domain, bits)?;
            if rows
                .iter()
                .any(|row: &SemanticCoverageRow| row.identity == identity)
            {
                return Err("duplicate exact compiler coordinate".into());
            }
            let record = sens::compiler_semantic_input_from_sens(identity)
                .map_err(|e| format!("SENS compiler law failed: {e:?}"))?
                .ok_or("SENS compiler law refused coordinate")?;
            if record.identity != identity
                || record.semantic_status != "current"
                || record.authority_path != "language-contract.lisp"
                || record.authority_ref.is_empty()
                || record.proof_ref.is_empty()
                || record.language_contract_version.is_empty()
                || record.authority_sha256 != authority_sha256
            {
                return Err("SENS compiler-law authority/proof mismatch".into());
            }
            if domain == "D3" {
                d3 += 1;
            } else {
                d4 += 1;
            }
            rows.push(SemanticCoverageRow {
                identity,
                execution_role: record.lowering_role,
                admission: AdmissionState::SourceAdmitted,
                operation_status: record.semantic_status,
                evidence: record.proof_ref,
                authority_sha256: record.authority_sha256,
                backend_evidence: Vec::new(), // no migrated native oracle yet
            });
        }
        if rows.len() != 9 || d3 != 7 || d4 != 2 {
            return Err("SENS compiler-role source subset drift (expected D3:7/D4:2)".into());
        }
        Ok(Self {
            upstream_channel: "current-sens-executable-compiler-law",
            authority_sha256,
            rows,
        })
    }

    /// Deliberately fail-closed for callers that previously used an implicit
    /// historical source registry; no fallback to retired eight-bit keys.
    pub fn build_source() -> Self {
        Self::try_current_sens().expect("BLOCK: exact SENS compiler coverage unavailable")
    }

    pub fn row(&self, identity: CoreDomainIdentity) -> Option<&SemanticCoverageRow> {
        self.rows.iter().find(|row| row.identity == identity)
    }

    pub fn summary(&self) -> CoverageSummary {
        let source_admitted = self
            .rows
            .iter()
            .filter(|r| r.admission == AdmissionState::SourceAdmitted)
            .count();
        let x86_executable = self
            .rows
            .iter()
            .flat_map(|r| &r.backend_evidence)
            .filter(|e| {
                e.backend == "x86-freestanding" && e.state == BackendEvidenceState::Executable
            })
            .count();
        let x86_assembly_witness_only = self
            .rows
            .iter()
            .flat_map(|r| &r.backend_evidence)
            .filter(|e| {
                e.backend == "x86-freestanding" && e.state == BackendEvidenceState::AssemblyWitness
            })
            .count();
        let x86_representation_limited = self
            .rows
            .iter()
            .flat_map(|r| &r.backend_evidence)
            .filter(|e| {
                e.backend == "x86-freestanding"
                    && e.state == BackendEvidenceState::ExecutableRepresentationLimited
            })
            .count();
        CoverageSummary {
            compiler_role_identities: self.rows.len(),
            source_admitted,
            not_yet_admitted: self.rows.len() - source_admitted,
            x86_executable,
            x86_assembly_witness_only,
            x86_representation_limited,
        }
    }
}
