//! Current SENS exact-domain -> CML bridge for #494.
//!
//! SENS owns identity, law, and the execution-role projection. This module
//! validates the pinned Contract 11.6 upstream source and then carries the
//! already-derived backend-neutral execution role into CML.
//!
//! No CML-owned D3 bits->meaning table exists here. Backends consume the
//! verified role and choose only their own private mechanism.

use crate::ir::Ir;
use std::fmt;

const UPSTREAM_REVISION_CHANNELS: &str = include_str!("../upstream-revisions.lisp");
const LANGUAGE_CONTRACT: &str = include_str!("../contracts/my-lisp/language-contract.lisp");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityProvenance {
    /// Exact supported SENS commit consumed by this CML checkout.
    pub upstream_sha: String,
    /// Current Contract major.minor carried by CML.
    pub contract_version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticRequest {
    pub identity: sens::CoreDomainIdentity,
    pub provenance: AuthorityProvenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedDomainCall {
    identity: sens::CoreDomainIdentity,
    role: sens::CompilerExecutionRole,
    args: Vec<Ir>,
}

impl VerifiedDomainCall {
    pub fn identity(&self) -> sens::CoreDomainIdentity {
        self.identity
    }

    /// Execution role already derived by the upstream SENS production law.
    pub fn role(&self) -> sens::CompilerExecutionRole {
        self.role
    }

    pub fn args(&self) -> &[Ir] {
        &self.args
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    MissingUpstreamPin,
    ContractVersionMismatch,
    AuthorityPinMismatch,
    D8ResearchRejected,
    UnsupportedDomainInFirstSlice,
    NoUpstreamExecutionRole,
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUpstreamPin => write!(f, "missing current SENS supported pin"),
            Self::ContractVersionMismatch => write!(f, "SENS Contract 11.6 is not active in CML"),
            Self::AuthorityPinMismatch => write!(f, "request does not match CML's exact SENS pin"),
            Self::D8ResearchRejected => write!(f, "D8 research identity rejected in production bridge"),
            Self::UnsupportedDomainInFirstSlice => {
                write!(f, "domain not admitted by the first D3 execution-role bridge slice")
            }
            Self::NoUpstreamExecutionRole => {
                write!(f, "SENS does not expose an admitted execution role for this identity")
            }
        }
    }
}

impl std::error::Error for BridgeError {}

fn quoted_field(source: &str, field: &str) -> Option<String> {
    let needle = format!("({field} . \"");
    let start = source.find(&needle)? + needle.len();
    let rest = &source[start..];
    let end = rest.find('\"')?;
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

pub fn pinned_authority() -> Result<AuthorityProvenance, BridgeError> {
    let upstream_sha = quoted_field(UPSTREAM_REVISION_CHANNELS, "supported-pin-sha")
        .ok_or(BridgeError::MissingUpstreamPin)?;
    let contract_version =
        contract_version().ok_or(BridgeError::MissingUpstreamPin)?;
    Ok(AuthorityProvenance {
        upstream_sha,
        contract_version,
    })
}

/// Validate current authority and ask SENS itself for the already-derived role.
pub fn verify_call(
    request: SemanticRequest,
    args: Vec<Ir>,
) -> Result<VerifiedDomainCall, BridgeError> {
    let pinned = pinned_authority()?;

    if pinned.contract_version != "11.6"
        || request.provenance.contract_version != pinned.contract_version
    {
        return Err(BridgeError::ContractVersionMismatch);
    }
    if request.provenance.upstream_sha != pinned.upstream_sha {
        return Err(BridgeError::AuthorityPinMismatch);
    }

    let role = match request.identity {
        sens::CoreDomainIdentity::D8(_) => return Err(BridgeError::D8ResearchRejected),
        sens::CoreDomainIdentity::D3(_) => sens::compiler_execution_role(request.identity)
            .ok_or(BridgeError::NoUpstreamExecutionRole)?,
        sens::CoreDomainIdentity::D4(_)
        | sens::CoreDomainIdentity::D5(_)
        | sens::CoreDomainIdentity::D6(_) => {
            return Err(BridgeError::UnsupportedDomainInFirstSlice)
        }
    };

    Ok(VerifiedDomainCall {
        identity: request.identity,
        role,
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d3(raw: u8) -> sens::CoreDomainIdentity {
        sens::CoreDomainIdentity::D3(sens::Bija3::from_word(
            sens::Bit3::new(raw).expect("3-bit test value"),
        ))
    }

    fn request(identity: sens::CoreDomainIdentity) -> SemanticRequest {
        SemanticRequest {
            identity,
            provenance: pinned_authority().expect("current pinned authority"),
        }
    }

    #[test]
    fn upstream_role_is_carried_without_local_bits_to_mechanism_mapping() {
        let call = verify_call(request(d3(0b100)), vec![Ir::Nil]).unwrap();
        assert_eq!(
            call.role(),
            sens::CompilerExecutionRole::SelectorHead
        );

        let call = verify_call(request(d3(0b011)), vec![Ir::Nil]).unwrap();
        assert_eq!(
            call.role(),
            sens::CompilerExecutionRole::SelectorTail
        );
    }

    #[test]
    fn unsupported_current_d3_role_fails_closed() {
        for raw in [0b000, 0b001, 0b010, 0b101, 0b110, 0b111] {
            assert_eq!(
                verify_call(request(d3(raw)), vec![]).unwrap_err(),
                BridgeError::NoUpstreamExecutionRole
            );
        }
    }

    #[test]
    fn stale_pin_and_research_domain_fail_closed() {
        let mut stale = request(d3(0b100));
        stale.provenance.upstream_sha = "deadbeef".into();
        assert_eq!(
            verify_call(stale, vec![]).unwrap_err(),
            BridgeError::AuthorityPinMismatch
        );

        let d8 = sens::CoreDomainIdentity::D8(sens::CoreD8::from_word(
            sens::Bit8::new(0b10000000).unwrap(),
        ));
        assert_eq!(
            verify_call(request(d8), vec![]).unwrap_err(),
            BridgeError::D8ResearchRejected
        );
    }
}
