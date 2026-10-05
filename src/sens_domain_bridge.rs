//! Межа current-domain SENS -> CML для compiler-in-language міграції (#494).
//!
//! Цей модуль НЕ визначає значення SENS-операцій і не містить доменних карт.
//! Він приймає SENS-owned `CoreDomainIdentity`, перевіряє pinned authority
//! provenance та повертає перевірений backend-neutral виклик.
//!
//! Конкретний backend уже після цієї межі може вибрати власний механізм.
//! У першому зрізі production-admission навмисно обмежений D3, щоб довести
//! CAR/CDR шлях без масового переписування legacy Sid8 IR.

use crate::ir::Ir;
use std::fmt;

const CONTRACT_LOCK: &str = include_str!("../contracts/my-lisp/lock.lisp");
const LANGUAGE_CONTRACT: &str = include_str!("../contracts/my-lisp/language-contract.lisp");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityProvenance {
    pub revision: String,
    pub sha256: String,
    pub contract_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticStatus {
    Current,
    Research,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MechanismStatus {
    Admitted,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticRequest {
    pub identity: sens::CoreDomainIdentity,
    pub law_ref: String,
    pub proof_ref: String,
    pub semantic_status: SemanticStatus,
    pub mechanism_status: MechanismStatus,
    pub provenance: AuthorityProvenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedDomainCall {
    identity: sens::CoreDomainIdentity,
    args: Vec<Ir>,
}

impl VerifiedDomainCall {
    pub fn identity(&self) -> sens::CoreDomainIdentity {
        self.identity
    }

    pub fn args(&self) -> &[Ir] {
        &self.args
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    MissingAuthorityField(&'static str),
    StaleAuthorityRevision,
    AuthorityDigestMismatch,
    ContractVersionMismatch,
    MissingLawReference,
    MissingProofReference,
    SemanticStatusNotCurrent,
    MechanismNotAdmitted,
    D8ResearchRejected,
    UnsupportedDomainInFirstSlice,
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAuthorityField(field) => {
                write!(f, "missing pinned SENS authority field: {field}")
            }
            Self::StaleAuthorityRevision => write!(f, "stale SENS authority revision"),
            Self::AuthorityDigestMismatch => write!(f, "SENS authority digest mismatch"),
            Self::ContractVersionMismatch => write!(f, "SENS contract version mismatch"),
            Self::MissingLawReference => write!(f, "missing SENS law reference"),
            Self::MissingProofReference => write!(f, "missing SENS proof reference"),
            Self::SemanticStatusNotCurrent => write!(f, "semantic status is not current"),
            Self::MechanismNotAdmitted => write!(f, "execution mechanism is not admitted"),
            Self::D8ResearchRejected => write!(f, "D8 research identity rejected in production bridge"),
            Self::UnsupportedDomainInFirstSlice => {
                write!(f, "domain not admitted by the first D3 bridge slice")
            }
        }
    }
}

impl std::error::Error for BridgeError {}

fn quoted_field(source: &str, field: &str) -> Option<String> {
    let needle = format!("({field} \"");
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

/// Повертає exact authority, яку CML реально pinned зараз.
/// Значення читаються з vendored lock/snapshot, а не дублюються Rust-константами.
pub fn pinned_authority() -> Result<AuthorityProvenance, BridgeError> {
    Ok(AuthorityProvenance {
        revision: quoted_field(CONTRACT_LOCK, "revision")
            .ok_or(BridgeError::MissingAuthorityField("revision"))?,
        sha256: quoted_field(CONTRACT_LOCK, "sha256")
            .ok_or(BridgeError::MissingAuthorityField("sha256"))?,
        contract_version: contract_version()
            .ok_or(BridgeError::MissingAuthorityField("contract-version"))?,
    })
}

/// Перевіряє semantic/provenance envelope ДО target selection.
///
/// Важливо: тут немає CAR/CDR таблиці. Bridge доводить лише:
/// - exact SENS-owned identity type;
/// - current/admitted request state;
/// - exact pinned authority;
/// - D3 scope першого vertical slice.
///
/// Який саме D3 resident CML уміє виконати — окремий backend mechanism fact.
pub fn verify_call(
    request: SemanticRequest,
    args: Vec<Ir>,
) -> Result<VerifiedDomainCall, BridgeError> {
    let pinned = pinned_authority()?;

    if request.provenance.revision != pinned.revision {
        return Err(BridgeError::StaleAuthorityRevision);
    }
    if request.provenance.sha256 != pinned.sha256 {
        return Err(BridgeError::AuthorityDigestMismatch);
    }
    if request.provenance.contract_version != pinned.contract_version {
        return Err(BridgeError::ContractVersionMismatch);
    }
    if request.law_ref.trim().is_empty() {
        return Err(BridgeError::MissingLawReference);
    }
    if request.proof_ref.trim().is_empty() {
        return Err(BridgeError::MissingProofReference);
    }
    if request.semantic_status != SemanticStatus::Current {
        return Err(BridgeError::SemanticStatusNotCurrent);
    }
    if request.mechanism_status != MechanismStatus::Admitted {
        return Err(BridgeError::MechanismNotAdmitted);
    }

    match request.identity {
        sens::CoreDomainIdentity::D8(_) => return Err(BridgeError::D8ResearchRejected),
        sens::CoreDomainIdentity::D3(_) => {}
        sens::CoreDomainIdentity::D4(_)
        | sens::CoreDomainIdentity::D5(_)
        | sens::CoreDomainIdentity::D6(_) => {
            return Err(BridgeError::UnsupportedDomainInFirstSlice);
        }
    }

    Ok(VerifiedDomainCall {
        identity: request.identity,
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

    fn current_request(identity: sens::CoreDomainIdentity) -> SemanticRequest {
        SemanticRequest {
            identity,
            law_ref: "language-contract.lisp:d3-foundation".into(),
            proof_ref: "contracts/bija3-l1-l5-ratification.lisp".into(),
            semantic_status: SemanticStatus::Current,
            mechanism_status: MechanismStatus::Admitted,
            provenance: pinned_authority().expect("pinned authority must parse"),
        }
    }

    #[test]
    fn exact_d3_identity_survives_verification_without_sid8_projection() {
        let call = verify_call(current_request(d3(0b100)), vec![Ir::Nil]).unwrap();
        assert_eq!(call.identity().width(), 3);
        assert_eq!(call.identity().packed_bits(), 0b100);
    }

    #[test]
    fn stale_revision_fails_before_backend_selection() {
        let mut request = current_request(d3(0b100));
        request.provenance.revision = "0000000000000000000000000000000000000000".into();
        assert_eq!(
            verify_call(request, vec![Ir::Nil]).unwrap_err(),
            BridgeError::StaleAuthorityRevision
        );
    }

    #[test]
    fn wrong_digest_fails_before_backend_selection() {
        let mut request = current_request(d3(0b011));
        request.provenance.sha256 = "00".repeat(32);
        assert_eq!(
            verify_call(request, vec![Ir::Nil]).unwrap_err(),
            BridgeError::AuthorityDigestMismatch
        );
    }

    #[test]
    fn same_payload_in_d4_is_not_accepted_as_d3() {
        let request = current_request(sens::CoreDomainIdentity::D4(
            sens::CoreD4::from_word(sens::Bit4::new(0b0100).unwrap()),
        ));
        assert_eq!(
            verify_call(request, vec![Ir::Nil]).unwrap_err(),
            BridgeError::UnsupportedDomainInFirstSlice
        );
    }

    #[test]
    fn d8_is_explicitly_fail_closed() {
        let request = current_request(sens::CoreDomainIdentity::D8(
            sens::CoreD8::from_word(sens::Bit8::new(0b00000100).unwrap()),
        ));
        assert_eq!(
            verify_call(request, vec![Ir::Nil]).unwrap_err(),
            BridgeError::D8ResearchRejected
        );
    }
}
