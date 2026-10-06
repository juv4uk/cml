//! Consumer for the SENS-owned compiler-semantic-input/1 transport.
//!
//! This layer parses transport only. It never derives a compiler role from an
//! identity. The exported exact identity and already-derived role are decoded
//! independently, then verify_rich_request re-checks both against the pinned
//! executable SENS law before CML chooses a private mechanism.

use crate::sens_domain_bridge::{AuthorityProvenance, MechanismStatus, SemanticStatus};
use crate::sens_rich_bridge::{
    RichBridgeError, RichSemanticRequest, VerifiedRichMechanism, verify_rich_request,
};
use std::fmt;

pub const SCHEMA: &str = "compiler-semantic-input/1";

const PINNED_COMPILER_NUCLEUS: &str = include_str!("../external/sens/lib/compiler-nucleus.lisp");

fn sha256_hex(bytes: &[u8]) -> String {
    sens::sha256_source(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedCompilerRequest {
    pub fixture_id: String,
    pub identity: sens::DomainIdentity,
    pub lowering_role: sens::CompilerLoweringRole,
    pub law_ref: String,
    pub proof_ref: String,
    pub provenance: AuthorityProvenance,
    pub compiler_nucleus_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilerExportError {
    MissingField(&'static str),
    WrongSchema(String),
    UnsupportedDomain(String),
    InvalidBits(String),
    UnknownRole(String),
    WrongSemanticStatus(String),
    WrongMechanismStatus(String),
    TargetMechanismLeaked,
    CompilerNucleusDigestMismatch,
    Malformed(String),
    Verification(RichBridgeError),
}

impl fmt::Display for CompilerExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CompilerExportError {}

impl From<RichBridgeError> for CompilerExportError {
    fn from(error: RichBridgeError) -> Self {
        Self::Verification(error)
    }
}

fn dotted_quoted(text: &str, field: &'static str) -> Result<String, CompilerExportError> {
    let marker = format!("({field} . \"");
    let start = text
        .find(&marker)
        .ok_or(CompilerExportError::MissingField(field))?
        + marker.len();
    let rest = &text[start..];
    let end = rest
        .find('"')
        .ok_or_else(|| CompilerExportError::Malformed(format!("unterminated {field}")))?;
    Ok(rest[..end].to_string())
}

fn dotted_symbol(text: &str, field: &'static str) -> Result<String, CompilerExportError> {
    let marker = format!("({field} . ");
    let start = text
        .find(&marker)
        .ok_or(CompilerExportError::MissingField(field))?
        + marker.len();
    let rest = &text[start..];
    let end = rest
        .find(|ch: char| ch == ')' || ch.is_whitespace())
        .ok_or_else(|| CompilerExportError::Malformed(format!("unterminated {field}")))?;
    Ok(rest[..end].to_string())
}

fn parse_identity(domain: &str, bits: &str) -> Result<sens::DomainIdentity, CompilerExportError> {
    if !bits.bytes().all(|byte| matches!(byte, b'0' | b'1')) {
        return Err(CompilerExportError::InvalidBits(bits.to_string()));
    }
    let raw = u8::from_str_radix(bits, 2)
        .map_err(|_| CompilerExportError::InvalidBits(bits.to_string()))?;

    match domain {
        "D3" if bits.len() == 3 => Ok(sens::DomainIdentity::D3(sens::Bija3::from_word(
            sens::Bit3::new(raw)
                .ok_or_else(|| CompilerExportError::InvalidBits(bits.to_string()))?,
        ))),
        "D4" if bits.len() == 4 => Ok(sens::DomainIdentity::D4(sens::CoreD4::from_word(
            sens::Bit4::new(raw)
                .ok_or_else(|| CompilerExportError::InvalidBits(bits.to_string()))?,
        ))),
        "D3" | "D4" => Err(CompilerExportError::InvalidBits(bits.to_string())),
        other => Err(CompilerExportError::UnsupportedDomain(other.to_string())),
    }
}

/// Decode the finite role tag carried by SENS. This is representation decoding,
/// not identity-to-meaning inference: identity is parsed independently above.
fn parse_role(tag: &str) -> Result<sens::CompilerLoweringRole, CompilerExportError> {
    match tag {
        "quote-form" => Ok(sens::CompilerLoweringRole::QuoteForm),
        "atom-predicate" => Ok(sens::CompilerLoweringRole::AtomPredicate),
        "selector-tail" => Ok(sens::CompilerLoweringRole::SelectorTail),
        "selector-head" => Ok(sens::CompilerLoweringRole::SelectorHead),
        "atom-equality" => Ok(sens::CompilerLoweringRole::AtomEquality),
        "cond-form" => Ok(sens::CompilerLoweringRole::CondForm),
        "pair-construct" => Ok(sens::CompilerLoweringRole::PairConstruct),
        "lambda-form" => Ok(sens::CompilerLoweringRole::LambdaForm),
        "define-form" => Ok(sens::CompilerLoweringRole::DefineForm),
        other => Err(CompilerExportError::UnknownRole(other.to_string())),
    }
}

fn parse_one(text: &str) -> Result<ExportedCompilerRequest, CompilerExportError> {
    let schema = dotted_symbol(text, "schema")?;
    if schema != SCHEMA {
        return Err(CompilerExportError::WrongSchema(schema));
    }

    let semantic_status = dotted_symbol(text, "semantic-status")?;
    if semantic_status != "current" {
        return Err(CompilerExportError::WrongSemanticStatus(semantic_status));
    }

    let mechanism_status = dotted_symbol(text, "mechanism-status")?;
    if mechanism_status != "unknown" {
        return Err(CompilerExportError::WrongMechanismStatus(mechanism_status));
    }
    if !text.contains("(mechanism-ref . ())") {
        return Err(CompilerExportError::TargetMechanismLeaked);
    }

    let domain = dotted_symbol(text, "domain")?;
    let bits = dotted_symbol(text, "bits")?;
    let role = dotted_symbol(text, "execution-role")?;
    let contract = dotted_symbol(text, "contract")?;

    Ok(ExportedCompilerRequest {
        fixture_id: dotted_quoted(text, "fixture-id")?,
        identity: parse_identity(&domain, &bits)?,
        lowering_role: parse_role(&role)?,
        law_ref: dotted_quoted(text, "authority-ref")?,
        proof_ref: dotted_quoted(text, "proof-ref")?,
        provenance: AuthorityProvenance {
            repository: dotted_quoted(text, "repository")?,
            revision: dotted_quoted(text, "revision")?,
            authority_path: dotted_quoted(text, "authority-path")?,
            authority_sha256: dotted_quoted(text, "authority-sha256")?,
            language_contract_version: contract,
        },
        compiler_nucleus_sha256: dotted_quoted(text, "compiler-nucleus-sha256")?,
    })
}

pub fn parse_compiler_export(
    text: &str,
) -> Result<Vec<ExportedCompilerRequest>, CompilerExportError> {
    let starts: Vec<_> = text
        .match_indices("(compiler-semantic-request")
        .map(|(index, _)| index)
        .collect();
    if starts.is_empty() {
        return Err(CompilerExportError::Malformed(
            "no compiler-semantic-request forms".into(),
        ));
    }

    let mut requests = Vec::with_capacity(starts.len());
    for (index, start) in starts.iter().copied().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(text.len());
        requests.push(parse_one(&text[start..end])?);
    }
    Ok(requests)
}

/// Admit an exported semantic request into CML's verified rich mechanism seam.
/// SENS exports mechanism-status=unknown; CML marks only its own private
/// mechanism availability as admitted here.
pub fn verify_exported_request(
    exported: ExportedCompilerRequest,
) -> Result<VerifiedRichMechanism, CompilerExportError> {
    if exported.compiler_nucleus_sha256.len() != 64
        || !exported
            .compiler_nucleus_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(CompilerExportError::Malformed(
            "invalid compiler nucleus digest".into(),
        ));
    }
    if exported.compiler_nucleus_sha256 != sha256_hex(PINNED_COMPILER_NUCLEUS.as_bytes()) {
        return Err(CompilerExportError::CompilerNucleusDigestMismatch);
    }

    Ok(verify_rich_request(RichSemanticRequest {
        identity: exported.identity,
        lowering_role: exported.lowering_role,
        law_ref: exported.law_ref,
        proof_ref: exported.proof_ref,
        semantic_status: SemanticStatus::Current,
        mechanism_status: MechanismStatus::Admitted,
        provenance: exported.provenance,
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substituted_compiler_nucleus_digest_fails_before_admission() {
        let mut request = ExportedCompilerRequest {
            fixture_id: "x".into(),
            identity: sens::DomainIdentity::D3(sens::Bija3::from_word(
                sens::Bit3::new(0b010).unwrap(),
            )),
            lowering_role: sens::CompilerLoweringRole::AtomPredicate,
            law_ref: "lib/compiler-nucleus.lisp:compiler-lowering-role-from-laws".into(),
            proof_ref: "contracts/bija3-l1-l5-ratification.lisp".into(),
            provenance: crate::sens_domain_bridge::pinned_authority().unwrap(),
            compiler_nucleus_sha256: sha256_hex(PINNED_COMPILER_NUCLEUS.as_bytes()),
        };
        request.compiler_nucleus_sha256 = "00".repeat(32);
        assert_eq!(
            verify_exported_request(request).unwrap_err(),
            CompilerExportError::CompilerNucleusDigestMismatch
        );
    }

    #[test]
    fn unsupported_width_fails_closed_before_role_verification() {
        let text = "(compiler-semantic-request
          (schema . compiler-semantic-input/1)
          (fixture-id . \"x\")
          (identity . ((domain . D8) (bits . 00000010)))
          (law . ((authority-ref . \"x\") (proof-ref . \"x\") (semantic-status . current)))
          (mechanism . ((execution-role . lambda-form) (mechanism-status . unknown) (mechanism-ref . ())))
          (provenance . ((repository . \"juv4uk/sens\") (revision . \"0000000000000000000000000000000000000000\") (authority-path . \"language-contract.lisp\") (authority-sha256 . \"0000000000000000000000000000000000000000000000000000000000000000\") (compiler-nucleus-sha256 . \"0000000000000000000000000000000000000000000000000000000000000000\") (contract . 11.6))))";
        assert!(matches!(
            parse_compiler_export(text),
            Err(CompilerExportError::UnsupportedDomain(domain)) if domain == "D8"
        ));
    }
}
