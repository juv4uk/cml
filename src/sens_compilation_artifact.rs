//! Consumer for the canonical SENS compiler-compilation-artifact/1 envelope.
//!
//! This module verifies transport/provenance only. It does not derive language
//! meaning, choose a CML mechanism, or install target code. The embedded exact
//! compiler-semantic-input/1 request is delegated to the already-existing
//! SENS export verifier before any downstream lowering is admitted.

use crate::sens_compiler_export::{
    CompilerExportError, ExportedCompilerRequest, parse_compiler_export, verify_exported_request,
};
use std::fmt;

pub const COMPILATION_ARTIFACT_SCHEMA: &str = "compiler-compilation-artifact/1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilationArtifactEnvelope {
    pub fixture_id: String,
    pub semantic_request_sha256: String,
    pub semantic_request: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCompilationArtifact {
    pub envelope: CompilationArtifactEnvelope,
    pub exported_request: ExportedCompilerRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCompilationExport {
    pub artifacts: Vec<VerifiedCompilationArtifact>,
    pub artifact_export_sha256: String,
    pub semantic_export: String,
    pub semantic_export_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilationArtifactError {
    MissingField(&'static str),
    WrongSchema(String),
    WrongArtifactStatus(String),
    RequiredCapabilitiesNotEmpty,
    InvalidSemanticRequestDigest,
    SemanticRequestDigestMismatch,
    FixtureMismatch,
    TargetPolicyLeaked(String),
    Malformed(String),
    SemanticRequest(CompilerExportError),
}

impl fmt::Display for CompilationArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CompilationArtifactError {}

impl From<CompilerExportError> for CompilationArtifactError {
    fn from(error: CompilerExportError) -> Self {
        Self::SemanticRequest(error)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    sens::sha256_source(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn dotted_quoted(text: &str, field: &'static str) -> Result<String, CompilationArtifactError> {
    let marker = format!("({field} . \"");
    let start = text
        .find(&marker)
        .ok_or(CompilationArtifactError::MissingField(field))?
        + marker.len();
    let rest = &text[start..];
    let mut escaped = false;
    for (offset, ch) in rest.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Ok(rest[..offset].to_string()),
            _ => {}
        }
    }
    Err(CompilationArtifactError::Malformed(format!(
        "unterminated quoted field {field}"
    )))
}

fn dotted_symbol(text: &str, field: &'static str) -> Result<String, CompilationArtifactError> {
    let marker = format!("({field} . ");
    let start = text
        .find(&marker)
        .ok_or(CompilationArtifactError::MissingField(field))?
        + marker.len();
    let rest = &text[start..];
    let end = rest
        .find(|ch: char| ch == ')' || ch.is_whitespace())
        .ok_or_else(|| CompilationArtifactError::Malformed(format!("unterminated {field}")))?;
    Ok(rest[..end].to_string())
}

fn extract_balanced_form(
    text: &str,
    marker: &'static str,
) -> Result<(usize, usize, String), CompilationArtifactError> {
    let marker_start = text
        .find(marker)
        .ok_or(CompilationArtifactError::MissingField("semantic-request"))?;
    let mut start = marker_start + marker.len();
    while text
        .as_bytes()
        .get(start)
        .is_some_and(u8::is_ascii_whitespace)
    {
        start += 1;
    }
    if text.as_bytes().get(start) != Some(&b'(') {
        return Err(CompilationArtifactError::Malformed(
            "semantic-request must contain one s-expression".into(),
        ));
    }

    let rest = &text[start..];
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in rest.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    CompilationArtifactError::Malformed(
                        "unbalanced semantic-request parentheses".into(),
                    )
                })?;
                if depth == 0 {
                    let end = start + offset + ch.len_utf8();
                    return Ok((start, end, text[start..end].to_string()));
                }
            }
            _ => {}
        }
    }

    Err(CompilationArtifactError::Malformed(
        "unterminated semantic-request form".into(),
    ))
}

fn parse_one(text: &str) -> Result<VerifiedCompilationArtifact, CompilationArtifactError> {
    let text = text.trim();
    if !text.starts_with("(compilation-artifact") {
        return Err(CompilationArtifactError::Malformed(
            "missing compilation-artifact envelope".into(),
        ));
    }

    let (request_start, request_end, semantic_request) =
        extract_balanced_form(text, "(semantic-request . ")?;
    let outer = format!("{}{}", &text[..request_start], &text[request_end..]);

    let schema = dotted_symbol(&outer, "schema")?;
    if schema != COMPILATION_ARTIFACT_SCHEMA {
        return Err(CompilationArtifactError::WrongSchema(schema));
    }

    let status = dotted_symbol(&outer, "artifact-status")?;
    if status != "canonical-backend-neutral" {
        return Err(CompilationArtifactError::WrongArtifactStatus(status));
    }
    if !outer.contains("(required-capabilities . ())") {
        return Err(CompilationArtifactError::RequiredCapabilitiesNotEmpty);
    }

    for token in ["cuda", "ptx", "cubin", "sass", "graal", "fpga", "slot-vm"] {
        if outer.to_ascii_lowercase().contains(token) {
            return Err(CompilationArtifactError::TargetPolicyLeaked(token.into()));
        }
    }

    let fixture_id = dotted_quoted(&outer, "fixture-id")?;
    let semantic_request_sha256 = dotted_quoted(&outer, "semantic-request-sha256")?;
    if !is_lower_hex(&semantic_request_sha256, 64) {
        return Err(CompilationArtifactError::InvalidSemanticRequestDigest);
    }
    if sha256_hex(semantic_request.as_bytes()) != semantic_request_sha256 {
        return Err(CompilationArtifactError::SemanticRequestDigestMismatch);
    }

    let exported = parse_compiler_export(&semantic_request)?;
    if exported.len() != 1 {
        return Err(CompilationArtifactError::Malformed(format!(
            "embedded semantic-request contains {} requests, expected 1",
            exported.len()
        )));
    }
    let exported_request = exported.into_iter().next().expect("one request");
    if exported_request.fixture_id != fixture_id {
        return Err(CompilationArtifactError::FixtureMismatch);
    }

    // Existing #622 verifier is the only semantic admission path.
    verify_exported_request(exported_request.clone())?;

    Ok(VerifiedCompilationArtifact {
        envelope: CompilationArtifactEnvelope {
            fixture_id,
            semantic_request_sha256,
            semantic_request,
        },
        exported_request,
    })
}

/// Validate one or more canonical SENS compilation artifacts.
///
/// The producer emits one top-level artifact per compiler-nucleus identity.
/// This function verifies every artifact and reconstructs the exact semantic
/// export that the already-merged current lowering registry consumes.
pub fn validate_artifact(
    text: &str,
) -> Result<VerifiedCompilationExport, CompilationArtifactError> {
    let starts: Vec<_> = text
        .match_indices("(compilation-artifact")
        .map(|(index, _)| index)
        .collect();
    if starts.is_empty() {
        return Err(CompilationArtifactError::Malformed(
            "no compilation-artifact forms".into(),
        ));
    }

    let mut artifacts = Vec::with_capacity(starts.len());
    for (index, start) in starts.iter().copied().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(text.len());
        artifacts.push(parse_one(&text[start..end])?);
    }

    let semantic_export = format!(
        "{}\n",
        artifacts
            .iter()
            .map(|artifact| artifact.envelope.semantic_request.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    );

    Ok(VerifiedCompilationExport {
        artifacts,
        artifact_export_sha256: sha256_hex(text.as_bytes()),
        semantic_export_sha256: sha256_hex(semantic_export.as_bytes()),
        semantic_export,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sens_current_lowering::VerifiedCurrentRegistry;
    use std::process::Command;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn pinned_artifact_export() -> &'static str {
        static EXPORT: OnceLock<String> = OnceLock::new();
        EXPORT
            .get_or_init(|| {
                let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
                let manifest = root.join("external/sens/Cargo.toml");
                let nonce = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock after epoch")
                    .as_nanos();
                let target = std::env::temp_dir().join(format!(
                    "cml-623-sens-artifact-{}-{nonce}",
                    std::process::id()
                ));

                let output = Command::new("cargo")
                    .current_dir(root.join("external/sens"))
                    .env("CARGO_TARGET_DIR", &target)
                    .args([
                        "run",
                        "--quiet",
                        "--manifest-path",
                        manifest.to_str().expect("UTF-8 SENS manifest path"),
                        "-p",
                        "xtask",
                        "--",
                        "compiler-export",
                        "--artifact",
                    ])
                    .output()
                    .expect("pinned SENS compiler artifact producer must execute");

                let _ = std::fs::remove_dir_all(&target);
                assert!(
                    output.status.success(),
                    "pinned SENS compiler artifact export failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout).expect("compiler artifact export is UTF-8")
            })
            .as_str()
    }

    #[test]
    fn real_producer_artifacts_verify_and_feed_existing_registry() {
        let verified = validate_artifact(pinned_artifact_export()).unwrap();
        assert_eq!(verified.artifacts.len(), 9);
        assert_eq!(verified.artifact_export_sha256.len(), 64);
        assert_eq!(verified.semantic_export_sha256.len(), 64);

        VerifiedCurrentRegistry::from_export(&verified.semantic_export)
            .expect("verified artifact export must feed the existing current registry");

        for artifact in &verified.artifacts {
            assert_eq!(
                sha256_hex(artifact.envelope.semantic_request.as_bytes()),
                artifact.envelope.semantic_request_sha256
            );
            assert_eq!(
                artifact.envelope.fixture_id,
                artifact.exported_request.fixture_id
            );
        }
    }

    #[test]
    fn altered_embedded_request_is_rejected_before_semantic_admission() {
        let source = pinned_artifact_export();
        let altered = source.replacen(
            "(domain . D3) (bits . 001)",
            "(domain . D3) (bits . 010)",
            1,
        );
        assert_ne!(altered, source);
        assert_eq!(
            validate_artifact(&altered).unwrap_err(),
            CompilationArtifactError::SemanticRequestDigestMismatch
        );
    }

    #[test]
    fn outer_fixture_must_match_embedded_request_fixture() {
        let source = pinned_artifact_export();
        let altered = source.replacen(
            "(fixture-id . \"nucleus-d3-001\")",
            "(fixture-id . \"outer-mismatch\")",
            1,
        );
        assert_ne!(altered, source);
        assert_eq!(
            validate_artifact(&altered).unwrap_err(),
            CompilationArtifactError::FixtureMismatch
        );
    }

    #[test]
    fn target_capability_smuggling_fails_closed() {
        let source = pinned_artifact_export();
        let altered = source.replacen(
            "(required-capabilities . ())",
            "(required-capabilities . (cuda))",
            1,
        );
        assert_ne!(altered, source);
        assert_eq!(
            validate_artifact(&altered).unwrap_err(),
            CompilationArtifactError::RequiredCapabilitiesNotEmpty
        );
    }
}
