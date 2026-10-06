//! Verified current-domain SENS lowering-role admission into CML rich mechanisms.
//!
//! SENS owns identity -> lowering-role meaning. CML verifies the pinned
//! authority/provenance, re-asks the pinned SENS executable law for the role,
//! compares the carried role, then chooses one CML-private mechanism through
//! the single #605 binding layer. No domain bits, surface names or legacy
//! callable identity are interpreted here.

use crate::compiler_mechanism::{RichCompilerMechanismRef, select_rich_compiler_mechanism};
use crate::sens_domain_bridge::{
    AuthorityProvenance, BridgeError, MechanismStatus, SemanticStatus,
    verify_authority_provenance,
};
use std::fmt;

const LANGUAGE_CONTRACT: &str = include_str!("../external/sens/language-contract.lisp");
const COMPILER_NUCLEUS: &str = include_str!("../external/sens/lib/compiler-nucleus.lisp");
const D3_PROOF: &str = include_str!("../external/sens/contracts/bija3-l1-l5-ratification.lisp");
const D4_PROOF: &str = include_str!("../external/sens/contracts/d4-bootstrap-ratification.lisp");

const COMPILER_ROLE_LAW_REF: &str =
    "lib/compiler-nucleus.lisp:compiler-lowering-role-from-laws";
const D3_PROOF_REF: &str = "contracts/bija3-l1-l5-ratification.lisp";
const D4_PROOF_REF: &str = "contracts/d4-bootstrap-ratification.lisp";

#[derive(Debug, Clone, PartialEq)]
pub struct RichSemanticRequest {
    pub identity: sens::DomainIdentity,
    pub lowering_role: sens::CompilerLoweringRole,
    pub law_ref: String,
    pub proof_ref: String,
    pub semantic_status: SemanticStatus,
    pub mechanism_status: MechanismStatus,
    pub provenance: AuthorityProvenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedRichMechanism {
    identity: sens::DomainIdentity,
    lowering_role: sens::CompilerLoweringRole,
    mechanism_ref: RichCompilerMechanismRef,
    provenance: AuthorityProvenance,
}

impl VerifiedRichMechanism {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichBridgeError {
    Authority(BridgeError),
    RoleProjectionFailure(String),
    UnsupportedOrResearchIdentity,
    UnsupportedLoweringRole,
    SemanticStatusNotCurrent,
    MechanismNotAdmitted,
    LoweringRoleMismatch,
}

impl fmt::Display for RichBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authority(error) => write!(formatter, "authority verification failed: {error}"),
            Self::RoleProjectionFailure(message) => {
                write!(formatter, "SENS lowering-role projection failed: {message}")
            }
            Self::UnsupportedOrResearchIdentity => {
                write!(
                    formatter,
                    "identity is not an admitted current Core identity"
                )
            }
            Self::UnsupportedLoweringRole => {
                write!(
                    formatter,
                    "identity is outside the current compiler-nucleus role closure"
                )
            }
            Self::SemanticStatusNotCurrent => write!(formatter, "semantic status is not current"),
            Self::MechanismNotAdmitted => write!(formatter, "mechanism status is not admitted"),
            Self::LoweringRoleMismatch => {
                write!(
                    formatter,
                    "carried lowering role disagrees with pinned SENS authority"
                )
            }
        }
    }
}

impl std::error::Error for RichBridgeError {}

impl From<BridgeError> for RichBridgeError {
    fn from(error: BridgeError) -> Self {
        Self::Authority(error)
    }
}

fn verify_role_law_and_proof(
    role: sens::CompilerLoweringRole,
    law_ref: &str,
    proof_ref: &str,
) -> Result<(), RichBridgeError> {
    if law_ref != COMPILER_ROLE_LAW_REF
        || !COMPILER_NUCLEUS.contains("(визначити compiler-lowering-role-from-laws")
        || !COMPILER_NUCLEUS.contains("compiler-lowering-role-from-laws")
        || !LANGUAGE_CONTRACT.contains("(d3-foundation")
        || !LANGUAGE_CONTRACT.contains("(d4-bootstrap")
    {
        return Err(RichBridgeError::Authority(BridgeError::UnknownLawReference));
    }

    let valid = match role {
        sens::CompilerLoweringRole::LambdaForm | sens::CompilerLoweringRole::DefineForm => {
            proof_ref == D4_PROOF_REF
                && D4_PROOF.contains("(status . owner-ratified)")
                && D4_PROOF.contains("(domain . D4)")
        }
        sens::CompilerLoweringRole::QuoteForm
        | sens::CompilerLoweringRole::AtomPredicate
        | sens::CompilerLoweringRole::SelectorTail
        | sens::CompilerLoweringRole::SelectorHead
        | sens::CompilerLoweringRole::AtomEquality
        | sens::CompilerLoweringRole::CondForm
        | sens::CompilerLoweringRole::PairConstruct => {
            proof_ref == D3_PROOF_REF
                && D3_PROOF.contains("(status . owner-ratified)")
                && D3_PROOF.contains("(domain . D3)")
        }
    };

    if valid {
        Ok(())
    } else {
        Err(RichBridgeError::Authority(BridgeError::UnknownProofReference))
    }
}

/// Ask the pinned SENS compiler law for the complete current nucleus role.
///
/// This is the only semantic admission query in the rich selfhost bridge.
pub fn authoritative_lowering_role(
    identity: sens::DomainIdentity,
) -> Result<sens::CompilerLoweringRole, RichBridgeError> {
    let core = identity
        .core_operation()
        .ok_or(RichBridgeError::UnsupportedOrResearchIdentity)?;
    sens::compiler_lowering_role_from_sens(core)
        .map_err(|error| RichBridgeError::RoleProjectionFailure(error.to_string()))?
        .ok_or(RichBridgeError::UnsupportedLoweringRole)
}

/// Verify one carried current-domain lowering request before IR construction.
pub fn verify_rich_request(
    request: RichSemanticRequest,
) -> Result<VerifiedRichMechanism, RichBridgeError> {
    if request.semantic_status != SemanticStatus::Current {
        return Err(RichBridgeError::SemanticStatusNotCurrent);
    }
    if request.mechanism_status != MechanismStatus::Admitted {
        return Err(RichBridgeError::MechanismNotAdmitted);
    }

    let pinned = verify_authority_provenance(&request.provenance)?;
    if request.law_ref.trim().is_empty() {
        return Err(RichBridgeError::Authority(BridgeError::MissingLawReference));
    }
    if request.proof_ref.trim().is_empty() {
        return Err(RichBridgeError::Authority(BridgeError::MissingProofReference));
    }
    verify_role_law_and_proof(
        request.lowering_role,
        &request.law_ref,
        &request.proof_ref,
    )?;

    let authoritative_role = authoritative_lowering_role(request.identity)?;

    if authoritative_role != request.lowering_role {
        return Err(RichBridgeError::LoweringRoleMismatch);
    }

    let mechanism_ref = select_rich_compiler_mechanism(authoritative_role);

    Ok(VerifiedRichMechanism {
        identity: request.identity,
        lowering_role: authoritative_role,
        mechanism_ref,
        provenance: pinned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sens_domain_bridge::pinned_authority;

    fn d3(raw: u8) -> sens::DomainIdentity {
        sens::DomainIdentity::D3(sens::Bija3::from_word(
            sens::Bit3::new(raw).expect("D3 test word"),
        ))
    }

    fn d4(raw: u8) -> sens::DomainIdentity {
        sens::DomainIdentity::D4(sens::CoreD4::from_word(
            sens::Bit4::new(raw).expect("D4 test word"),
        ))
    }

    fn current(identity: sens::DomainIdentity) -> RichSemanticRequest {
        RichSemanticRequest {
            identity,
            lowering_role: authoritative_lowering_role(identity)
                .expect("test identity has SENS-derived selfhost role"),
            law_ref: COMPILER_ROLE_LAW_REF.into(),
            proof_ref: match identity {
                sens::DomainIdentity::D4(_) => D4_PROOF_REF.into(),
                _ => D3_PROOF_REF.into(),
            },
            semantic_status: SemanticStatus::Current,
            mechanism_status: MechanismStatus::Admitted,
            provenance: pinned_authority().expect("pinned authority parses"),
        }
    }

    #[test]
    fn verified_atom_role_reaches_only_current_d1_atom_mechanism() {
        let verified = verify_rich_request(current(d3(0b010))).unwrap();
        assert_eq!(
            verified.lowering_role(),
            sens::CompilerLoweringRole::AtomPredicate
        );
        assert_eq!(
            verified.mechanism_ref(),
            RichCompilerMechanismRef::AtomPredicateD1
        );
    }

    #[test]
    fn verified_d4_lambda_role_reaches_only_rich_lambda_mechanism() {
        let verified = verify_rich_request(current(d4(0b0010))).unwrap();
        assert_eq!(
            verified.lowering_role(),
            sens::CompilerLoweringRole::LambdaForm
        );
        assert_eq!(verified.mechanism_ref(), RichCompilerMechanismRef::Lambda);
    }

    #[test]
    fn law_and_proof_refs_are_checked_before_rich_mechanism_selection() {
        let mut request = current(d3(0b110));
        request.law_ref = "wrong-law".into();
        assert_eq!(
            verify_rich_request(request).unwrap_err(),
            RichBridgeError::Authority(BridgeError::UnknownLawReference)
        );

        let mut request = current(d4(0b0010));
        request.proof_ref = D3_PROOF_REF.into();
        assert_eq!(
            verify_rich_request(request).unwrap_err(),
            RichBridgeError::Authority(BridgeError::UnknownProofReference)
        );
    }

    #[test]
    fn carried_role_cannot_override_sens_authority() {
        let mut request = current(d3(0b010));
        request.lowering_role = sens::CompilerLoweringRole::QuoteForm;
        assert_eq!(
            verify_rich_request(request).unwrap_err(),
            RichBridgeError::LoweringRoleMismatch
        );
    }

    #[test]
    fn stale_authority_fails_before_mechanism_selection() {
        let mut request = current(d4(0b0011));
        request.provenance.revision = "0".repeat(40);
        assert_eq!(
            verify_rich_request(request).unwrap_err(),
            RichBridgeError::Authority(BridgeError::StaleAuthorityRevision)
        );
    }

    #[test]
    fn d8_fails_closed_before_any_rich_mechanism() {
        let identity = sens::DomainIdentity::D8(sens::CoreD8::from_word(
            sens::Bit8::new(0b0000_0010).unwrap(),
        ));
        assert_eq!(
            authoritative_lowering_role(identity).unwrap_err(),
            RichBridgeError::UnsupportedOrResearchIdentity
        );
    }

    #[test]
    fn bridge_source_has_no_identity_decode_or_legacy_semantic_route() {
        let source = include_str!("sens_rich_bridge.rs");
        let forbidden = [
            ["packed", "_bits("].concat(),
            ["Sid", "8"].concat(),
            ["Sens", "8"].concat(),
            ["compiler_execution_", "role("].concat(),
            ["domain_identity_for_", "surface"].concat(),
        ];
        for forbidden in &forbidden {
            assert!(
                !source.contains(forbidden),
                "rich bridge reintroduced forbidden semantic route: {forbidden}"
            );
        }
        assert!(source.contains("sens::compiler_lowering_role_from_sens("));
        assert!(source.contains("select_rich_compiler_mechanism(authoritative_role)"));
    }
}
