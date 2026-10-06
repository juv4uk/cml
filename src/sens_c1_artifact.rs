//! Executable C1 artifact bundle for the current SENS compiler nucleus.
//!
//! The bundle carries the exact SENS source, the exact proof-bearing
//! compiler-semantic-input/1 export, verified SENS authority provenance,
//! and the generated C source. No language meaning is reconstructed here.

use crate::c_backend::CBackend;
use crate::sens_current_lowering::{
    CurrentLowerError, VerifiedCurrentRegistry, lower_current_sens_source,
};
use crate::sens_domain_bridge::AuthorityProvenance;
use std::fmt;

const MAGIC: &[u8; 8] = b"CMLSENS1";
const PINNED_NUCLEUS: &str = include_str!("../external/sens/lib/compiler-nucleus.lisp");

pub const C1_BACKEND_ID: &str = "cml.c/current-domain";
pub const C1_ARTIFACT_FORMAT: &str = "CMLSENS-C1-1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentSensC1Artifact {
    pub source: String,
    pub source_sha256: String,
    pub compiler_export: String,
    pub compiler_export_sha256: String,
    pub authority: AuthorityProvenance,
    pub backend_id: String,
    pub artifact_format: String,
    pub c_source: String,
    pub c_source_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum C1ArtifactError {
    Lower(CurrentLowerError),
    Backend(String),
    WrongSourceBundle,
    InvalidSourceSha,
    InvalidCompilerExportSha,
    InvalidCSourceSha,
    InvalidAuthority,
    WrongBackend,
    WrongFormat,
    InvalidMagic,
    InvalidUtf8,
    Truncated,
}

impl fmt::Display for C1ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for C1ArtifactError {}

impl From<CurrentLowerError> for C1ArtifactError {
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

fn validate(artifact: &CurrentSensC1Artifact) -> Result<(), C1ArtifactError> {
    if artifact.source != PINNED_NUCLEUS {
        return Err(C1ArtifactError::WrongSourceBundle);
    }
    if !valid_hex(&artifact.source_sha256, 64)
        || artifact.source_sha256 != sha256_hex(artifact.source.as_bytes())
    {
        return Err(C1ArtifactError::InvalidSourceSha);
    }
    if !valid_hex(&artifact.compiler_export_sha256, 64)
        || artifact.compiler_export_sha256 != sha256_hex(artifact.compiler_export.as_bytes())
    {
        return Err(C1ArtifactError::InvalidCompilerExportSha);
    }
    if !valid_hex(&artifact.c_source_sha256, 64)
        || artifact.c_source_sha256 != sha256_hex(artifact.c_source.as_bytes())
    {
        return Err(C1ArtifactError::InvalidCSourceSha);
    }

    let registry = VerifiedCurrentRegistry::from_export(&artifact.compiler_export)?;
    if registry.authority() != &artifact.authority
        || artifact.authority.repository != "juv4uk/sens"
        || !valid_hex(&artifact.authority.revision, 40)
        || !valid_hex(&artifact.authority.authority_sha256, 64)
        || artifact.authority.authority_path != "language-contract.lisp"
    {
        return Err(C1ArtifactError::InvalidAuthority);
    }
    if artifact.backend_id != C1_BACKEND_ID {
        return Err(C1ArtifactError::WrongBackend);
    }
    if artifact.artifact_format != C1_ARTIFACT_FORMAT {
        return Err(C1ArtifactError::WrongFormat);
    }
    Ok(())
}

/// Compile the exact current SENS compiler nucleus through the verified export
/// boundary and package the resulting executable-C C1 source with its evidence.
pub fn build_current_sens_c1(
    source: &str,
    compiler_export: &str,
) -> Result<CurrentSensC1Artifact, C1ArtifactError> {
    if source != PINNED_NUCLEUS {
        return Err(C1ArtifactError::WrongSourceBundle);
    }

    let lowered = lower_current_sens_source(source, compiler_export)?;
    let mut backend = CBackend::new();
    let c_source = backend
        .compile_program(&lowered.ir)
        .map_err(|error| C1ArtifactError::Backend(error.to_string()))?;

    let artifact = CurrentSensC1Artifact {
        source: source.to_string(),
        source_sha256: sha256_hex(source.as_bytes()),
        compiler_export: compiler_export.to_string(),
        compiler_export_sha256: sha256_hex(compiler_export.as_bytes()),
        authority: lowered.authority,
        backend_id: C1_BACKEND_ID.to_string(),
        artifact_format: C1_ARTIFACT_FORMAT.to_string(),
        c_source_sha256: sha256_hex(c_source.as_bytes()),
        c_source,
    };
    validate(&artifact)?;
    Ok(artifact)
}

impl CurrentSensC1Artifact {
    pub fn encode_v1(&self) -> Result<Vec<u8>, C1ArtifactError> {
        validate(self)?;
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        push(&mut out, &self.source);
        push(&mut out, &self.source_sha256);
        push(&mut out, &self.compiler_export);
        push(&mut out, &self.compiler_export_sha256);
        push(&mut out, &self.authority.repository);
        push(&mut out, &self.authority.revision);
        push(&mut out, &self.authority.authority_path);
        push(&mut out, &self.authority.authority_sha256);
        push(&mut out, &self.authority.language_contract_version);
        push(&mut out, &self.backend_id);
        push(&mut out, &self.artifact_format);
        push(&mut out, &self.c_source);
        push(&mut out, &self.c_source_sha256);
        Ok(out)
    }

    pub fn decode_v1(bytes: &[u8]) -> Result<Self, C1ArtifactError> {
        if bytes.len() < MAGIC.len() {
            return Err(C1ArtifactError::Truncated);
        }
        let mut decoder = Decoder { bytes, offset: 0 };
        if decoder.take(MAGIC.len())? != MAGIC {
            return Err(C1ArtifactError::InvalidMagic);
        }

        let artifact = Self {
            source: decoder.take_string()?,
            source_sha256: decoder.take_string()?,
            compiler_export: decoder.take_string()?,
            compiler_export_sha256: decoder.take_string()?,
            authority: AuthorityProvenance {
                repository: decoder.take_string()?,
                revision: decoder.take_string()?,
                authority_path: decoder.take_string()?,
                authority_sha256: decoder.take_string()?,
                language_contract_version: decoder.take_string()?,
            },
            backend_id: decoder.take_string()?,
            artifact_format: decoder.take_string()?,
            c_source: decoder.take_string()?,
            c_source_sha256: decoder.take_string()?,
        };
        if decoder.offset != bytes.len() {
            return Err(C1ArtifactError::Truncated);
        }
        validate(&artifact)?;
        Ok(artifact)
    }
}

fn push(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], C1ArtifactError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(C1ArtifactError::Truncated)?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(C1ArtifactError::Truncated)?;
        self.offset = end;
        Ok(slice)
    }

    fn take_string(&mut self) -> Result<String, C1ArtifactError> {
        let len = self.take_u32()? as usize;
        let raw = self.take(len)?;
        std::str::from_utf8(raw)
            .map(str::to_string)
            .map_err(|_| C1ArtifactError::InvalidUtf8)
    }

    fn take_u32(&mut self) -> Result<u32, C1ArtifactError> {
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

    fn pinned_compiler_export() -> &'static str {
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
                    "cml-604-sens-export-{}-{nonce}",
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
                    ])
                    .output()
                    .expect("pinned SENS compiler-export must execute");

                let _ = std::fs::remove_dir_all(&target);
                assert!(
                    output.status.success(),
                    "pinned SENS compiler-export failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout).expect("compiler export is UTF-8")
            })
            .as_str()
    }

    #[test]
    fn c1_bundle_carries_source_export_proof_and_provenance() {
        let export = pinned_compiler_export();
        let artifact =
            build_current_sens_c1(SOURCE, export).expect("current nucleus C1 artifact");

        assert_eq!(artifact.source, SOURCE);
        assert_eq!(artifact.backend_id, C1_BACKEND_ID);
        assert_eq!(artifact.artifact_format, C1_ARTIFACT_FORMAT);
        assert_eq!(
            artifact.authority.revision,
            "f2e7797283c8dfc2aa67935a02b3735a8290041f"
        );
        assert!(artifact.compiler_export.contains("(proof-ref . "));
        assert!(
            artifact
                .compiler_export
                .contains("contracts/bija3-l1-l5-ratification.lisp")
        );
        assert!(
            artifact
                .compiler_export
                .contains("contracts/d4-bootstrap-ratification.lisp")
        );
        assert!(artifact.compiler_export.contains("(domain . D3)"));
        assert!(artifact.compiler_export.contains("(domain . D4)"));

        assert!(artifact.c_source.contains("v_atom_predicate("));
        assert!(artifact.c_source.contains("v_eq_predicate("));
        assert!(artifact.c_source.contains("require_predicate_bit("));
        assert!(artifact.c_source.contains("require_tag(_v, TAG_CONS, \"car\")"));
        assert!(artifact.c_source.contains("require_tag(_v, TAG_CONS, \"cdr\")"));
        assert!(!artifact.c_source.contains("mk_sid_callable(0b"));

        let decoded =
            CurrentSensC1Artifact::decode_v1(&artifact.encode_v1().unwrap()).unwrap();
        assert_eq!(decoded, artifact);
    }

    #[test]
    fn c1_source_compiles_and_executes() {
        let artifact = build_current_sens_c1(SOURCE, pinned_compiler_export())
            .expect("current nucleus C1 artifact");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "cml-sens-c1-{}-{nonce}",
            std::process::id()
        ));
        let source_path = base.with_extension("c");
        let binary_path = base.with_extension("bin");
        std::fs::write(&source_path, &artifact.c_source).unwrap();

        let compile = Command::new("gcc")
            .arg(&source_path)
            .arg("-o")
            .arg(&binary_path)
            .output()
            .expect("gcc must execute");
        assert!(
            compile.status.success(),
            "generated C1 source did not compile: {}",
            String::from_utf8_lossy(&compile.stderr)
        );

        let run = Command::new(&binary_path).output().expect("C1 executable must run");
        assert!(
            run.status.success(),
            "generated C1 executable failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(binary_path);
    }

    #[test]
    fn same_payload_wrong_domain_cannot_inherit_mechanism() {
        let export = pinned_compiler_export();
        assert!(export.contains("(domain . D3) (bits . 010)"));
        assert!(export.contains("(domain . D4) (bits . 0010)"));

        let tampered = export.replacen(
            "(domain . D3) (bits . 010)",
            "(domain . D4) (bits . 0010)",
            1,
        );
        assert_ne!(tampered, export);

        assert!(
            build_current_sens_c1(SOURCE, &tampered).is_err(),
            "same payload under the wrong domain must fail before C1 artifact emission"
        );
    }

    #[test]
    fn modified_source_cannot_reuse_current_nucleus_proof_export() {
        let modified = format!("{SOURCE}\n; modified");
        assert_eq!(
            build_current_sens_c1(&modified, pinned_compiler_export()).unwrap_err(),
            C1ArtifactError::WrongSourceBundle
        );
    }
}
