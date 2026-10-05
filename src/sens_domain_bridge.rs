//! Proof-carrying exact-domain SENS -> CML compiler boundary (#494).
//!
//! SENS owns semantic identity and identity -> execution-role projection.
//! CML verifies the pinned authority bundle, validates the carried role against
//! the SENS API, then binds that already-verified role to a private target
//! mechanism. No domain coordinate is decoded in this module.

use crate::compiler_mechanism::{
    CompilerMechanismRef, RichCompilerMechanismRef, select_rich_compiler_mechanism,
    select_slot_vm_mechanism,
};
use crate::ir::Ir;
use std::fmt;

const UPSTREAM_REVISIONS: &str = include_str!("../upstream-revisions.lisp");
const LANGUAGE_CONTRACT: &str = include_str!("../external/sens/language-contract.lisp");
const COMPILER_INPUT_CONTRACT: &str =
    include_str!("../external/sens/contracts/compiler-semantic-input-v1.lisp");
const COMPILER_NUCLEUS: &str = include_str!("../external/sens/lib/compiler-nucleus.lisp");
const D3_PROOF: &str = include_str!("../external/sens/contracts/bija3-l1-l5-ratification.lisp");
const D4_PROOF: &str = include_str!("../external/sens/contracts/d4-bootstrap-ratification.lisp");

const SENS_REPOSITORY: &str = "juv4uk/sens";
const AUTHORITY_PATH: &str = "language-contract.lisp";
const D3_LAW_REF: &str = "language-contract.lisp:d3-foundation";
const D3_PROOF_REF: &str = "contracts/bija3-l1-l5-ratification.lisp";
const D4_LAW_REF: &str = "language-contract.lisp:d4-bootstrap";
const D4_PROOF_REF: &str = "contracts/d4-bootstrap-ratification.lisp";
const COMPILER_ROLE_LAW_REF: &str =
    "lib/compiler-nucleus.lisp:compiler-lowering-role-from-laws";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityProvenance {
    pub repository: String,
    pub revision: String,
    pub authority_path: String,
    pub authority_sha256: String,
    pub language_contract_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticStatus {
    Current,
    Research,
    Unallocated,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MechanismStatus {
    Admitted,
    Blocked,
    NotRequired,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticRequest {
    pub identity: sens::DomainIdentity,
    pub execution_role: sens::CompilerExecutionRole,
    pub law_ref: String,
    pub proof_ref: String,
    pub semantic_status: SemanticStatus,
    pub mechanism_status: MechanismStatus,
    pub provenance: AuthorityProvenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedDomainMechanism {
    identity: sens::DomainIdentity,
    execution_role: sens::CompilerExecutionRole,
    mechanism_ref: CompilerMechanismRef,
    provenance: AuthorityProvenance,
}

impl VerifiedDomainMechanism {
    pub const fn identity(&self) -> sens::DomainIdentity {
        self.identity
    }

    pub const fn execution_role(&self) -> sens::CompilerExecutionRole {
        self.execution_role
    }

    pub const fn mechanism_ref(&self) -> CompilerMechanismRef {
        self.mechanism_ref
    }

    pub fn provenance(&self) -> &AuthorityProvenance {
        &self.provenance
    }
}

#[derive(Debug, Clone, PartialEq)]
#[derive(Debug, Clone, PartialEq)]
pub struct CompilerLoweringRequest {
    pub identity: sens::DomainIdentity,
    pub lowering_role: sens::CompilerLoweringRole,
    pub law_ref: String,
    pub proof_ref: String,
    pub semantic_status: SemanticStatus,
    pub mechanism_status: MechanismStatus,
    pub provenance: AuthorityProvenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedRichCompilerMechanism {
    identity: sens::DomainIdentity,
    lowering_role: sens::CompilerLoweringRole,
    mechanism_ref: RichCompilerMechanismRef,
    provenance: AuthorityProvenance,
}

impl VerifiedRichCompilerMechanism {
    pub const fn identity(&self) -> sens::DomainIdentity {
        self.identity
    }

    pub const fn lowering_role(&self) -> sens::CompilerLoweringRole {
        self.lowering_role
    }

    pub const fn mechanism_ref(&self) -> RichCompilerMechanismRef {
        self.mechanism_ref
    }

    pub fn provenance(&self) -> &AuthorityProvenance {
        &self.provenance
    }
}

pub struct VerifiedDomainCall {
    identity: sens::DomainIdentity,
    execution_role: sens::CompilerExecutionRole,
    mechanism_ref: CompilerMechanismRef,
    provenance: AuthorityProvenance,
    args: Vec<Ir>,
}

impl VerifiedDomainCall {
    pub const fn identity(&self) -> sens::DomainIdentity {
        self.identity
    }

    pub const fn execution_role(&self) -> sens::CompilerExecutionRole {
        self.execution_role
    }

    pub const fn mechanism_ref(&self) -> CompilerMechanismRef {
        self.mechanism_ref
    }

    pub fn provenance(&self) -> &AuthorityProvenance {
        &self.provenance
    }

    pub fn args(&self) -> &[Ir] {
        &self.args
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    MissingAuthorityField(&'static str),
    PinChannelMismatch,
    RepositoryMismatch,
    StaleAuthorityRevision,
    AuthorityPathMismatch,
    AuthorityDigestMismatch,
    ContractVersionMismatch,
    MissingLawReference,
    MissingProofReference,
    UnknownLawReference,
    UnknownProofReference,
    SemanticStatusNotCurrent,
    MechanismNotAdmitted,
    UnsupportedOrResearchIdentity,
    UnsupportedExecutionRole,
    RoleProjectionFailure(String),
    ExecutionRoleMismatch,
    UpstreamBoundaryContractMissing,
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAuthorityField(field) => {
                write!(formatter, "missing pinned SENS authority field: {field}")
            }
            Self::PinChannelMismatch => {
                write!(formatter, "CML build-source and supported SENS pins differ")
            }
            Self::RepositoryMismatch => write!(formatter, "SENS authority repository mismatch"),
            Self::StaleAuthorityRevision => write!(formatter, "stale SENS authority revision"),
            Self::AuthorityPathMismatch => write!(formatter, "SENS authority path mismatch"),
            Self::AuthorityDigestMismatch => write!(formatter, "SENS authority digest mismatch"),
            Self::ContractVersionMismatch => write!(formatter, "SENS contract version mismatch"),
            Self::MissingLawReference => write!(formatter, "missing SENS law reference"),
            Self::MissingProofReference => write!(formatter, "missing SENS proof reference"),
            Self::UnknownLawReference => write!(formatter, "unknown SENS law reference"),
            Self::UnknownProofReference => write!(formatter, "unknown SENS proof reference"),
            Self::SemanticStatusNotCurrent => write!(formatter, "semantic status is not current"),
            Self::MechanismNotAdmitted => write!(formatter, "execution mechanism is not admitted"),
            Self::UnsupportedOrResearchIdentity => {
                write!(
                    formatter,
                    "identity is not an admitted callable Core identity"
                )
            }
            Self::UnsupportedExecutionRole => {
                write!(
                    formatter,
                    "identity has no execution role in the first compiler slice"
                )
            }
            Self::RoleProjectionFailure(message) => {
                write!(formatter, "SENS compiler role projection failed: {message}")
            }
            Self::ExecutionRoleMismatch => {
                write!(
                    formatter,
                    "carried execution role disagrees with pinned SENS authority"
                )
            }
            Self::UpstreamBoundaryContractMissing => {
                write!(
                    formatter,
                    "pinned SENS compiler boundary contract is incomplete"
                )
            }
        }
    }
}

impl std::error::Error for BridgeError {}

fn dotted_quoted_field(source: &str, field: &str) -> Option<String> {
    let needle = format!("({field} . \"");
    let start = source.find(&needle)? + needle.len();
    let rest = &source[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn contract_version() -> Option<String> {
    let major_marker = "(major . #d";
    let minor_marker = "(minor . ";
    let major_start = LANGUAGE_CONTRACT.find(major_marker)? + major_marker.len();
    let major_tail = &LANGUAGE_CONTRACT[major_start..];
    let major_end = major_tail.find(')')?;
    let minor_start = LANGUAGE_CONTRACT.find(minor_marker)? + minor_marker.len();
    let minor_tail = &LANGUAGE_CONTRACT[minor_start..];
    let minor_end = minor_tail.find(')')?;
    Some(format!(
        "{}.{}",
        major_tail[..major_end].trim(),
        minor_tail[..minor_end].trim()
    ))
}

fn sha256_hex(bytes: &[u8]) -> String {
    sens::sha256_source(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Exact authority bundle corresponding to CML's checked-in SENS gitlink.
///
/// Revision comes from `upstream-revisions.lisp`; digest/version are computed
/// from the exact files compiled through `external/sens`.
pub fn pinned_authority() -> Result<AuthorityProvenance, BridgeError> {
    let build_source = dotted_quoted_field(UPSTREAM_REVISIONS, "build-source-sha")
        .ok_or(BridgeError::MissingAuthorityField("build-source-sha"))?;
    let supported = dotted_quoted_field(UPSTREAM_REVISIONS, "supported-pin-sha")
        .ok_or(BridgeError::MissingAuthorityField("supported-pin-sha"))?;
    if build_source != supported {
        return Err(BridgeError::PinChannelMismatch);
    }

    Ok(AuthorityProvenance {
        repository: SENS_REPOSITORY.to_string(),
        revision: supported,
        authority_path: AUTHORITY_PATH.to_string(),
        authority_sha256: sha256_hex(LANGUAGE_CONTRACT.as_bytes()),
        language_contract_version: contract_version().ok_or(BridgeError::MissingAuthorityField(
            "language-contract-version",
        ))?,
    })
}

fn verify_boundary_contract() -> Result<(), BridgeError> {
    let required = [
        "(compiler-may-infer-meaning . no)",
        "(backend-may-infer-meaning . no)",
        "(execution-role . required)",
        "(mechanism-ref . required-when-admitted)",
        "(verify-before-lowering . required)",
    ];
    if required
        .iter()
        .all(|needle| COMPILER_INPUT_CONTRACT.contains(needle))
    {
        Ok(())
    } else {
        Err(BridgeError::UpstreamBoundaryContractMissing)
    }
}

/// Ask the pinned SENS compiler law for the authoritative bounded execution role.
///
/// CML does not inspect domain coordinates and never falls back to the Rust
/// differential oracle if SENS evaluation fails.
pub fn authoritative_execution_role(
    identity: sens::DomainIdentity,
) -> Result<sens::CompilerExecutionRole, BridgeError> {
    let core = identity
        .core_operation()
        .ok_or(BridgeError::UnsupportedOrResearchIdentity)?;
    sens::compiler_execution_role_from_sens(core)
        .map_err(|error| BridgeError::RoleProjectionFailure(error.to_string()))?
        .ok_or(BridgeError::UnsupportedExecutionRole)
}

/// Ask the pinned SENS compiler nucleus for the authoritative full lowering role.
///
/// CML performs no identity/bit/name role lookup. The nine-role meaning comes
/// from the executable SENS-owned compiler law introduced by sens#3824/#3826.
pub fn authoritative_lowering_role(
    identity: sens::DomainIdentity,
) -> Result<sens::CompilerLoweringRole, BridgeError> {
    let core = identity
        .core_operation()
        .ok_or(BridgeError::UnsupportedOrResearchIdentity)?;
    sens::compiler_lowering_role_from_sens(core)
        .map_err(|error| BridgeError::RoleProjectionFailure(error.to_string()))?
        .ok_or(BridgeError::UnsupportedExecutionRole)
}

fn verify_lowering_law_and_proof(
    role: sens::CompilerLoweringRole,
    law_ref: &str,
    proof_ref: &str,
) -> Result<(), BridgeError> {
    if law_ref != COMPILER_ROLE_LAW_REF
        || !COMPILER_NUCLEUS.contains("(compiler-lowering-role-from-laws")
    {
        return Err(BridgeError::UnknownLawReference);
    }

    match role {
        sens::CompilerLoweringRole::LambdaForm | sens::CompilerLoweringRole::DefineForm => {
            if proof_ref != D4_PROOF_REF
                || !D4_PROOF.contains("(status . owner-ratified)")
                || !D4_PROOF.contains("(domain . D4)")
                || !LANGUAGE_CONTRACT.contains("(d4-bootstrap")
            {
                return Err(BridgeError::UnknownProofReference);
            }
        }
        sens::CompilerLoweringRole::QuoteForm
        | sens::CompilerLoweringRole::AtomPredicate
        | sens::CompilerLoweringRole::SelectorTail
        | sens::CompilerLoweringRole::SelectorHead
        | sens::CompilerLoweringRole::AtomEquality
        | sens::CompilerLoweringRole::CondForm
        | sens::CompilerLoweringRole::PairConstruct => {
            if proof_ref != D3_PROOF_REF
                || !D3_PROOF.contains("(status . owner-ratified)")
                || !D3_PROOF.contains("(domain . D3)")
                || !LANGUAGE_CONTRACT.contains("(d3-foundation")
            {
                return Err(BridgeError::UnknownProofReference);
            }
        }
    }

    Ok(())
}

/// Verify one full compiler lowering request and bind it to an existing rich
/// mechanism. The SENS role is checked again immediately before CML mapping.
pub fn verify_lowering_request(
    request: CompilerLoweringRequest,
) -> Result<VerifiedRichCompilerMechanism, BridgeError> {
    verify_boundary_contract()?;
    let pinned = pinned_authority()?;

    if request.provenance.repository != pinned.repository {
        return Err(BridgeError::RepositoryMismatch);
    }
    if request.provenance.revision != pinned.revision {
        return Err(BridgeError::StaleAuthorityRevision);
    }
    if request.provenance.authority_path != pinned.authority_path {
        return Err(BridgeError::AuthorityPathMismatch);
    }
    if request.provenance.authority_sha256 != pinned.authority_sha256 {
        return Err(BridgeError::AuthorityDigestMismatch);
    }
    if request.provenance.language_contract_version != pinned.language_contract_version {
        return Err(BridgeError::ContractVersionMismatch);
    }
    if request.semantic_status != SemanticStatus::Current {
        return Err(BridgeError::SemanticStatusNotCurrent);
    }
    if request.mechanism_status != MechanismStatus::Admitted {
        return Err(BridgeError::MechanismNotAdmitted);
    }

    verify_lowering_law_and_proof(
        request.lowering_role,
        &request.law_ref,
        &request.proof_ref,
    )?;

    let authoritative_role = authoritative_lowering_role(request.identity)?;
    if authoritative_role != request.lowering_role {
        return Err(BridgeError::ExecutionRoleMismatch);
    }

    let mechanism_ref = select_rich_compiler_mechanism(authoritative_role);

    Ok(VerifiedRichCompilerMechanism {
        identity: request.identity,
        lowering_role: authoritative_role,
        mechanism_ref,
        provenance: pinned,
    })
}

/// Validate one SENS semantic request before any target/backend execution.
///
/// The identity -> role decision is executed by the pinned SENS-written law.
/// CML only checks that the carried role agrees, then binds it to a mechanism.
pub fn verify_request(request: SemanticRequest) -> Result<VerifiedDomainMechanism, BridgeError> {
    verify_boundary_contract()?;
    let pinned = pinned_authority()?;

    if request.provenance.repository != pinned.repository {
        return Err(BridgeError::RepositoryMismatch);
    }
    if request.provenance.revision != pinned.revision {
        return Err(BridgeError::StaleAuthorityRevision);
    }
    if request.provenance.authority_path != pinned.authority_path {
        return Err(BridgeError::AuthorityPathMismatch);
    }
    if request.provenance.authority_sha256 != pinned.authority_sha256 {
        return Err(BridgeError::AuthorityDigestMismatch);
    }
    if request.provenance.language_contract_version != pinned.language_contract_version {
        return Err(BridgeError::ContractVersionMismatch);
    }

    if request.law_ref.trim().is_empty() {
        return Err(BridgeError::MissingLawReference);
    }
    if request.proof_ref.trim().is_empty() {
        return Err(BridgeError::MissingProofReference);
    }
    if request.law_ref != D3_LAW_REF || !LANGUAGE_CONTRACT.contains("(d3-foundation") {
        return Err(BridgeError::UnknownLawReference);
    }
    if request.proof_ref != D3_PROOF_REF
        || !D3_PROOF.contains("(status . owner-ratified)")
        || !D3_PROOF.contains("(domain . D3)")
    {
        return Err(BridgeError::UnknownProofReference);
    }

    if request.semantic_status != SemanticStatus::Current {
        return Err(BridgeError::SemanticStatusNotCurrent);
    }
    if request.mechanism_status != MechanismStatus::Admitted {
        return Err(BridgeError::MechanismNotAdmitted);
    }

    let authoritative_role = authoritative_execution_role(request.identity)?;

    if authoritative_role != request.execution_role {
        return Err(BridgeError::ExecutionRoleMismatch);
    }

    let mechanism_ref = select_slot_vm_mechanism(authoritative_role);

    Ok(VerifiedDomainMechanism {
        identity: request.identity,
        execution_role: authoritative_role,
        mechanism_ref,
        provenance: pinned,
    })
}

/// Compatibility wrapper for callers that already have lowered arguments.
///
/// Verification is deliberately args-independent: semantic admission and
/// role/mechanism selection happen before target-specific argument lowering.
pub fn verify_call(
    request: SemanticRequest,
    args: Vec<Ir>,
) -> Result<VerifiedDomainCall, BridgeError> {
    let verified = verify_request(request)?;

    Ok(VerifiedDomainCall {
        identity: verified.identity,
        execution_role: verified.execution_role,
        mechanism_ref: verified.mechanism_ref,
        provenance: verified.provenance,
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d3(raw: u8) -> sens::DomainIdentity {
        sens::DomainIdentity::D3(sens::Bija3::from_word(
            sens::Bit3::new(raw).expect("valid D3 test identity"),
        ))
    }

    fn d4(raw: u8) -> sens::DomainIdentity {
        sens::DomainIdentity::D4(sens::CoreD4::from_word(
            sens::Bit4::new(raw).expect("valid D4 test identity"),
        ))
    }

    fn d8(raw: u8) -> sens::DomainIdentity {
        sens::DomainIdentity::D8(sens::CoreD8::from_word(
            sens::Bit8::new(raw).expect("valid D8 test identity"),
        ))
    }

    fn role(identity: sens::DomainIdentity) -> sens::CompilerExecutionRole {
        authoritative_execution_role(identity)
            .expect("test identity has SENS-derived compiler role")
    }

    fn current_request(identity: sens::DomainIdentity) -> SemanticRequest {
        SemanticRequest {
            identity,
            execution_role: role(identity),
            law_ref: D3_LAW_REF.to_string(),
            proof_ref: D3_PROOF_REF.to_string(),
            semantic_status: SemanticStatus::Current,
            mechanism_status: MechanismStatus::Admitted,
            provenance: pinned_authority().expect("pinned authority must parse"),
        }
    }

    fn lowering_role(identity: sens::DomainIdentity) -> sens::CompilerLoweringRole {
        authoritative_lowering_role(identity)
            .expect("test identity has SENS-derived lowering role")
    }

    fn current_lowering_request(
        identity: sens::DomainIdentity,
    ) -> CompilerLoweringRequest {
        let role = lowering_role(identity);
        let (law_ref, proof_ref) = match role {
            sens::CompilerLoweringRole::LambdaForm
            | sens::CompilerLoweringRole::DefineForm => {
                (D4_LAW_REF, D4_PROOF_REF)
            }
            _ => (D3_LAW_REF, D3_PROOF_REF),
        };

        CompilerLoweringRequest {
            identity,
            lowering_role: role,
            law_ref: COMPILER_ROLE_LAW_REF.into(),
            proof_ref: proof_ref.into(),
            semantic_status: SemanticStatus::Current,
            mechanism_status: MechanismStatus::Admitted,
            provenance: pinned_authority().unwrap(),
        }
    }

    #[test]
    fn all_current_nucleus_roles_are_verified_by_sens_before_rich_binding() {
        let cases = [
            (d3(0b001), sens::CompilerLoweringRole::QuoteForm),
            (d3(0b010), sens::CompilerLoweringRole::AtomPredicate),
            (d3(0b011), sens::CompilerLoweringRole::SelectorTail),
            (d3(0b100), sens::CompilerLoweringRole::SelectorHead),
            (d3(0b101), sens::CompilerLoweringRole::AtomEquality),
            (d3(0b110), sens::CompilerLoweringRole::CondForm),
            (d3(0b111), sens::CompilerLoweringRole::PairConstruct),
            (d4(0b0010), sens::CompilerLoweringRole::LambdaForm),
            (d4(0b0011), sens::CompilerLoweringRole::DefineForm),
        ];

        for (identity, expected) in cases {
            let verified =
                verify_lowering_request(current_lowering_request(identity)).unwrap();
            assert_eq!(verified.identity(), identity);
            assert_eq!(verified.lowering_role(), expected);
        }
    }

    #[test]
    fn rich_role_binding_is_mechanism_only() {
        let expected = [
            (d3(0b001), "cml.rich.quote"),
            (d3(0b010), "cml.rich.atom-predicate-d1"),
            (d3(0b011), "cml.rich.cdr"),
            (d3(0b100), "cml.rich.car"),
            (d3(0b101), "cml.rich.atom-equality-d1"),
            (d3(0b110), "cml.rich.cond-exact-d1"),
            (d3(0b111), "cml.rich.cons"),
            (d4(0b0010), "cml.rich.lambda"),
            (d4(0b0011), "cml.rich.define"),
        ];

        for (identity, mechanism) in expected {
            let verified =
                verify_lowering_request(current_lowering_request(identity)).unwrap();
            assert_eq!(verified.mechanism_ref().as_str(), mechanism);
        }
    }

    #[test]
    fn wrong_domain_payload_cannot_inherit_a_d3_rich_role() {
        let identity = d4(0b0010);
        let request = CompilerLoweringRequest {
            identity,
            lowering_role: sens::CompilerLoweringRole::QuoteForm,
            law_ref: COMPILER_ROLE_LAW_REF.into(),
            proof_ref: D3_PROOF_REF.into(),
            semantic_status: SemanticStatus::Current,
            mechanism_status: MechanismStatus::Admitted,
            provenance: pinned_authority().unwrap(),
        };

        assert_eq!(
            verify_lowering_request(request).unwrap_err(),
            BridgeError::ExecutionRoleMismatch
        );
    }

    #[test]
    fn stale_lowering_provenance_fails_before_rich_mechanism_selection() {
        let mut request = current_lowering_request(d3(0b110));
        request.provenance.revision = "0".repeat(40);
        assert_eq!(
            verify_lowering_request(request).unwrap_err(),
            BridgeError::StaleAuthorityRevision
        );

        let mut request = current_lowering_request(d4(0b0010));
        request.provenance.authority_sha256 = "00".repeat(32);
        assert_eq!(
            verify_lowering_request(request).unwrap_err(),
            BridgeError::AuthorityDigestMismatch
        );
    }

    #[test]
    fn pinned_authority_is_computed_from_current_gitlink_and_contract_bytes() {
        let authority = pinned_authority().unwrap();
        assert_eq!(authority.repository, "juv4uk/sens");
        assert_eq!(authority.revision.len(), 40);
        assert_eq!(authority.authority_path, "language-contract.lisp");
        assert_eq!(authority.authority_sha256.len(), 64);
        assert_eq!(authority.language_contract_version, "11.6");
    }

    #[test]
    fn pinned_authority_revision_matches_checked_out_sens_gitlink() {
        let authority = pinned_authority().expect("pinned authority must parse");
        let sens_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("external/sens");
        let output = std::process::Command::new("git")
            .arg("-c")
            .arg(format!("safe.directory={}", sens_dir.display()))
            .arg("-C")
            .arg(&sens_dir)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("focused pure-Guix evidence environment must provide git");
        assert!(
            output.status.success(),
            "git rev-parse failed for {}: {}",
            sens_dir.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = String::from_utf8(output.stdout)
            .expect("git SHA must be UTF-8")
            .trim()
            .to_owned();
        assert_eq!(
            actual, authority.revision,
            "proof-carrying provenance revision must equal the exact external/sens gitlink checkout"
        );
    }

    #[test]
    fn verified_pair_construct_role_selects_only_cml_private_cons_mechanism() {
        let verified = verify_request(current_request(d3(0b111))).unwrap();
        assert_eq!(
            verified.execution_role(),
            sens::CompilerExecutionRole::PairConstruct
        );
        assert_eq!(verified.mechanism_ref(), CompilerMechanismRef::SlotVmCons);
        assert_eq!(verified.mechanism_ref().as_str(), "cml.slot-vm.cons");
    }

    #[test]
    fn verified_head_role_selects_only_cml_private_slot_mechanism() {
        let call = verify_call(current_request(d3(0b100)), vec![Ir::Nil]).unwrap();
        assert_eq!(
            call.execution_role(),
            sens::CompilerExecutionRole::SelectorHead
        );
        assert_eq!(call.mechanism_ref(), CompilerMechanismRef::SlotVmCar);
        assert_eq!(call.mechanism_ref().as_str(), "cml.slot-vm.car");
    }

    #[test]
    fn verified_tail_role_selects_only_cml_private_slot_mechanism() {
        let call = verify_call(current_request(d3(0b011)), vec![Ir::Nil]).unwrap();
        assert_eq!(
            call.execution_role(),
            sens::CompilerExecutionRole::SelectorTail
        );
        assert_eq!(call.mechanism_ref(), CompilerMechanismRef::SlotVmCdr);
        assert_eq!(call.mechanism_ref().as_str(), "cml.slot-vm.cdr");
    }

    #[test]
    fn stale_or_wrong_authority_fails_before_mechanism_selection() {
        let mut request = current_request(d3(0b100));
        request.provenance.revision = "0".repeat(40);
        assert_eq!(
            verify_call(request, vec![Ir::Nil]).unwrap_err(),
            BridgeError::StaleAuthorityRevision
        );

        let mut request = current_request(d3(0b100));
        request.provenance.authority_sha256 = "00".repeat(32);
        assert_eq!(
            verify_call(request, vec![Ir::Nil]).unwrap_err(),
            BridgeError::AuthorityDigestMismatch
        );
    }

    #[test]
    fn carried_role_must_equal_the_role_projected_by_sens() {
        let mut request = current_request(d3(0b100));
        request.execution_role = sens::CompilerExecutionRole::SelectorTail;
        assert_eq!(
            verify_call(request, vec![Ir::Nil]).unwrap_err(),
            BridgeError::ExecutionRoleMismatch
        );
    }

    #[test]
    fn same_payload_in_wider_domain_does_not_become_d3_selector() {
        let identity = d4(0b0100);
        let request = SemanticRequest {
            identity,
            execution_role: sens::CompilerExecutionRole::SelectorHead,
            law_ref: D3_LAW_REF.to_string(),
            proof_ref: D3_PROOF_REF.to_string(),
            semantic_status: SemanticStatus::Current,
            mechanism_status: MechanismStatus::Admitted,
            provenance: pinned_authority().unwrap(),
        };
        assert_eq!(
            verify_call(request, vec![Ir::Nil]).unwrap_err(),
            BridgeError::UnsupportedExecutionRole
        );
    }

    #[test]
    fn d8_research_identity_fails_closed_even_if_caller_claims_current_admitted() {
        let request = SemanticRequest {
            identity: d8(0b1000_0000),
            execution_role: sens::CompilerExecutionRole::SelectorHead,
            law_ref: D3_LAW_REF.to_string(),
            proof_ref: D3_PROOF_REF.to_string(),
            semantic_status: SemanticStatus::Current,
            mechanism_status: MechanismStatus::Admitted,
            provenance: pinned_authority().unwrap(),
        };
        assert_eq!(
            verify_call(request, vec![Ir::Nil]).unwrap_err(),
            BridgeError::UnsupportedOrResearchIdentity
        );
    }

    #[test]
    fn production_bridge_source_does_not_call_rust_role_oracle() {
        let source = include_str!("sens_domain_bridge.rs");
        let forbidden = [
            "sens::compiler_execution_role(",
            "sens::compiler_lowering_role(",
            "core.bits",
            "packed_bits",
        ]
        .concat();
        assert!(
            !source.contains(&forbidden),
            "production bridge must not reconstruct SENS role meaning locally"
        );
        assert!(source.contains("sens::compiler_execution_role_from_sens("));
        assert!(source.contains("sens::compiler_lowering_role_from_sens("));
    }

    #[test]
    fn blocked_mechanism_fails_before_target_selection() {
        let mut request = current_request(d3(0b011));
        request.mechanism_status = MechanismStatus::Blocked;
        assert_eq!(
            verify_call(request, vec![Ir::Nil]).unwrap_err(),
            BridgeError::MechanismNotAdmitted
        );
    }
}
