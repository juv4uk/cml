//! Executable C0 artifact boundary for the current SENS compiler nucleus.
//!
//! SENS remains the semantic authority. This module only:
//! - consumes the already-verified SENS compiler-semantic-input/1 export;
//! - lowers through the current CML mechanism-only IR;
//! - records exact SENS + CML provenance;
//! - serializes the resulting native C source deterministically.
//!
//! No semantic role table, coordinate decoding, Sid8/Sens8 fallback, or target
//! meaning is introduced here.

use crate::c_backend::CBackend;
use crate::sens_current_lowering::{CurrentLowerError, lower_current_sens_source};
use crate::sens_domain_bridge::AuthorityProvenance;
use std::fmt;

const MAGIC: &[u8; 8] = b"CMLSENS0";
const MAX_FIELD_LEN: usize = 16 * 1024 * 1024;

pub const C0_BACKEND_ID: &str = "cml.c";
pub const C0_ARTIFACT_FORMAT: &str = "CMLSENS-C0-1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentSensC0Artifact {
    pub source_sha256: String,
    pub authority: AuthorityProvenance,
    pub cml_revision: String,
    pub backend_id: String,
    pub artifact_format: String,
    pub c_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum C0ArtifactError {
    Lower(CurrentLowerError),
    CCompile(String),
    InvalidSha,
    InvalidAuthority,
    InvalidCmlRevision,
    WrongBackend,
    WrongFormat,
    InvalidMagic,
    InvalidUtf8,
    Truncated,
    FieldTooLarge,
}

impl fmt::Display for C0ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lower(error) => write!(f, "current SENS lowering failed: {error}"),
            Self::CCompile(error) => write!(f, "generated C source compilation failed: {error}"),
            Self::InvalidSha => write!(f, "invalid C0 source SHA-256"),
            Self::InvalidAuthority => write!(f, "invalid SENS authority provenance"),
            Self::InvalidCmlRevision => write!(f, "invalid CML revision"),
            Self::WrongBackend => write!(f, "wrong C0 backend id"),
            Self::WrongFormat => write!(f, "wrong C0 artifact format"),
            Self::InvalidMagic => write!(f, "invalid C0 artifact magic"),
            Self::InvalidUtf8 => write!(f, "invalid UTF-8 C0 artifact field"),
            Self::Truncated => write!(f, "truncated C0 artifact"),
            Self::FieldTooLarge => write!(f, "C0 artifact field exceeds safety bound"),
        }
    }
}

impl std::error::Error for C0ArtifactError {}

impl From<CurrentLowerError> for C0ArtifactError {
    fn from(error: CurrentLowerError) -> Self {
        Self::Lower(error)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    sens::sha256_source(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn valid_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn validate(artifact: &CurrentSensC0Artifact) -> Result<(), C0ArtifactError> {
    if !valid_hex(&artifact.source_sha256, 64) {
        return Err(C0ArtifactError::InvalidSha);
    }
    if artifact.authority.repository != "juv4uk/sens"
        || !valid_hex(&artifact.authority.revision, 40)
        || !valid_hex(&artifact.authority.authority_sha256, 64)
        || artifact.authority.authority_path != "language-contract.lisp"
        || artifact.authority.language_contract_version != "11.6"
    {
        return Err(C0ArtifactError::InvalidAuthority);
    }
    if !valid_hex(&artifact.cml_revision, 40) {
        return Err(C0ArtifactError::InvalidCmlRevision);
    }
    if artifact.backend_id != C0_BACKEND_ID {
        return Err(C0ArtifactError::WrongBackend);
    }
    if artifact.artifact_format != C0_ARTIFACT_FORMAT {
        return Err(C0ArtifactError::WrongFormat);
    }
    for field in [
        &artifact.source_sha256,
        &artifact.authority.repository,
        &artifact.authority.revision,
        &artifact.authority.authority_path,
        &artifact.authority.authority_sha256,
        &artifact.authority.language_contract_version,
        &artifact.cml_revision,
        &artifact.backend_id,
        &artifact.artifact_format,
        &artifact.c_source,
    ] {
        if field.len() > MAX_FIELD_LEN {
            return Err(C0ArtifactError::FieldTooLarge);
        }
    }
    Ok(())
}

/// Build the C0 artifact from the real, already-produced SENS semantic export.
///
/// cml_revision is recorded provenance, not a source of language meaning.
pub fn build_current_sens_c0(
    source: &str,
    compiler_export: &str,
    cml_revision: &str,
) -> Result<CurrentSensC0Artifact, C0ArtifactError> {
    let lowered = lower_current_sens_source(source, compiler_export)?;
    let mut backend = CBackend::new();
    let c_source = backend
        .compile_program(&lowered.ir)
        .map_err(|error| C0ArtifactError::CCompile(error.to_string()))?;

    let artifact = CurrentSensC0Artifact {
        source_sha256: sha256_hex(source.as_bytes()),
        authority: lowered.authority,
        cml_revision: cml_revision.to_string(),
        backend_id: C0_BACKEND_ID.to_string(),
        artifact_format: C0_ARTIFACT_FORMAT.to_string(),
        c_source,
    };
    validate(&artifact)?;
    Ok(artifact)
}

impl CurrentSensC0Artifact {
    pub fn encode_v1(&self) -> Result<Vec<u8>, C0ArtifactError> {
        validate(self)?;
        let mut out = Vec::with_capacity(self.c_source.len() + 512);
        out.extend_from_slice(MAGIC);
        push(&mut out, &self.source_sha256)?;
        push(&mut out, &self.authority.repository)?;
        push(&mut out, &self.authority.revision)?;
        push(&mut out, &self.authority.authority_path)?;
        push(&mut out, &self.authority.authority_sha256)?;
        push(&mut out, &self.authority.language_contract_version)?;
        push(&mut out, &self.cml_revision)?;
        push(&mut out, &self.backend_id)?;
        push(&mut out, &self.artifact_format)?;
        push(&mut out, &self.c_source)?;
        Ok(out)
    }

    pub fn decode_v1(bytes: &[u8]) -> Result<Self, C0ArtifactError> {
        if bytes.len() < MAGIC.len() {
            return Err(C0ArtifactError::Truncated);
        }
        let mut decoder = Decoder { bytes, offset: 0 };
        if decoder.take(MAGIC.len())? != MAGIC {
            return Err(C0ArtifactError::InvalidMagic);
        }
        let artifact = Self {
            source_sha256: decoder.take_string()?,
            authority: AuthorityProvenance {
                repository: decoder.take_string()?,
                revision: decoder.take_string()?,
                authority_path: decoder.take_string()?,
                authority_sha256: decoder.take_string()?,
                language_contract_version: decoder.take_string()?,
            },
            cml_revision: decoder.take_string()?,
            backend_id: decoder.take_string()?,
            artifact_format: decoder.take_string()?,
            c_source: decoder.take_string()?,
        };
        if decoder.offset != bytes.len() {
            return Err(C0ArtifactError::Truncated);
        }
        validate(&artifact)?;
        Ok(artifact)
    }
}

fn push(out: &mut Vec<u8>, value: &str) -> Result<(), C0ArtifactError> {
    if value.len() > MAX_FIELD_LEN || value.len() > u32::MAX as usize {
        return Err(C0ArtifactError::FieldTooLarge);
    }
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], C0ArtifactError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(C0ArtifactError::Truncated)?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(C0ArtifactError::Truncated)?;
        self.offset = end;
        Ok(slice)
    }

    fn take_string(&mut self) -> Result<String, C0ArtifactError> {
        let len = self.take_u32()? as usize;
        if len > MAX_FIELD_LEN {
            return Err(C0ArtifactError::FieldTooLarge);
        }
        let raw = self.take(len)?;
        std::str::from_utf8(raw)
            .map(str::to_string)
            .map_err(|_| C0ArtifactError::InvalidUtf8)
    }

    fn take_u32(&mut self) -> Result<u32, C0ArtifactError> {
        let raw = self.take(4)?;
        Ok(u32::from_le_bytes(
            raw.try_into().expect("exact four-byte slice"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SOURCE: &str = include_str!("../external/sens/lib/compiler-nucleus.lisp");
    const TEST_CML_REVISION: &str = "3fda3cdb087a24b3e124c811abff90803028518c";

    fn pinned_compiler_export() -> &'static str {
        static EXPORT: OnceLock<String> = OnceLock::new();
        EXPORT
            .get_or_init(|| {
                let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
                let output = Command::new("cargo")
                    .current_dir(root.join("external/sens"))
                    .args(["run", "--quiet", "-p", "xtask", "--", "compiler-export"])
                    .output()
                    .expect("pinned SENS compiler-export must execute");
                assert!(
                    output.status.success(),
                    "pinned SENS compiler-export failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout).expect("compiler export is UTF-8")
            })
            .as_str()
    }

    fn build() -> CurrentSensC0Artifact {
        build_current_sens_c0(SOURCE, pinned_compiler_export(), TEST_CML_REVISION)
            .expect("current SENS C0 artifact")
    }

    #[test]
    fn c0_artifact_carries_exact_source_and_provenance() {
        let artifact = build();
        assert_eq!(artifact.backend_id, C0_BACKEND_ID);
        assert_eq!(artifact.artifact_format, C0_ARTIFACT_FORMAT);
        assert_eq!(artifact.cml_revision, TEST_CML_REVISION);
        assert_eq!(
            artifact.authority.revision,
            "f2e7797283c8dfc2aa67935a02b3735a8290041f"
        );
        assert_eq!(artifact.authority.language_contract_version, "11.6");
        assert!(artifact.c_source.contains("v_atom_predicate("));
        assert!(artifact.c_source.contains("v_eq_predicate("));
        assert!(artifact.c_source.contains("require_predicate_bit("));
        assert!(artifact.c_source.contains("mk_cons("));
        assert!(
            artifact
                .c_source
                .contains("require_tag(_v, TAG_CONS, \"car\")")
        );
        assert!(
            artifact
                .c_source
                .contains("require_tag(_v, TAG_CONS, \"cdr\")")
        );
        assert!(!artifact.c_source.contains("mk_sid_callable(0b"));

        let encoded = artifact.encode_v1().expect("C0 artifact encoding");
        let decoded = CurrentSensC0Artifact::decode_v1(&encoded).expect("C0 artifact decoding");
        assert_eq!(decoded, artifact);
    }

    #[test]
    fn c0_artifact_c_source_compiles_and_executes() {
        let artifact = build();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("cml-sens-c0-{}-{nonce}", std::process::id()));
        let source_path = base.with_extension("c");
        let binary_path = base.with_extension("bin");
        std::fs::write(&source_path, &artifact.c_source).expect("write generated C0 source");

        let compile = Command::new("gcc")
            .arg(&source_path)
            .arg("-o")
            .arg(&binary_path)
            .output()
            .expect("gcc must be available on the C0 runner");
        assert!(
            compile.status.success(),
            "generated C0 source did not compile: {}",
            String::from_utf8_lossy(&compile.stderr)
        );

        let run = Command::new(&binary_path)
            .output()
            .expect("generated C0 executable must run");
        assert!(
            run.status.success(),
            "generated C0 executable failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );

        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(binary_path);
    }
}
