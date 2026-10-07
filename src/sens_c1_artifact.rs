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
const MAX_FIELD_LEN: usize = 16 * 1024 * 1024;

pub const C1_BACKEND_ID: &str = "cml.c/current-domain";
pub const C1_ARTIFACT_FORMAT: &str = "CMLSENS-C1-1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentSensC1Artifact {
    pub source: String,
    pub source_sha256: String,
    pub compiler_export: String,
    pub compiler_export_sha256: String,
    pub authority: AuthorityProvenance,
    pub cml_revision: String,
    pub backend_id: String,
    pub artifact_format: String,
    pub c_source: String,
    pub c_source_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum C1ArtifactError {
    Lower(CurrentLowerError),
    Backend(String),
    Bootstrap(String),
    WrongSourceBundle,
    InvalidSourceSha,
    InvalidCompilerExportSha,
    InvalidCSourceSha,
    InvalidAuthority,
    InvalidCmlRevision,
    WrongBackend,
    WrongFormat,
    InvalidMagic,
    InvalidUtf8,
    Truncated,
    FieldTooLarge,
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

fn c_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("mk_string(\"{escaped}\")")
}

fn c_list(items: &[String]) -> String {
    items
        .iter()
        .rev()
        .fold("(&NIL_V)".to_string(), |tail, head| {
            format!("mk_cons({head}, {tail})")
        })
}

fn c_value_expr(value: &sens::Value) -> Result<String, C1ArtifactError> {
    match value {
        sens::Value::Nil => Ok("(&NIL_V)".to_string()),
        sens::Value::Number(number, sens::Exactness::Exact)
            if number.is_finite()
                && number.fract() == 0.0
                && *number >= i64::MIN as f64
                && *number <= i64::MAX as f64 =>
        {
            Ok(format!("mk_int({})", *number as i64))
        }
        sens::Value::DomainIdentity(identity) if identity.width() == 1 => {
            Ok(format!("mk_predicate_bit({})", identity.packed_bits()))
        }
        sens::Value::DomainIdentity(identity) => Ok(format!(
            "mk_domain_identity({}, {})",
            identity.width(),
            identity.packed_bits()
        )),
        sens::Value::String(text) => Ok(c_string(text)),
        sens::Value::Symbol(symbol) => {
            let escaped = symbol.replace('\\', "\\\\").replace('"', "\\\"");
            Ok(format!("mk_sym(\"{escaped}\")"))
        }
        sens::Value::Pair(head, tail) => Ok(format!(
            "mk_cons({}, {})",
            c_value_expr(head)?,
            c_value_expr(tail)?
        )),
        other => Err(C1ArtifactError::Bootstrap(format!(
            "unsupported verified SENS bootstrap value for C initializer: {other}"
        ))),
    }
}

fn c1_driver_body(
    authority: &AuthorityProvenance,
    bundle: &sens::CompilerProgramBootstrapBundle,
) -> Result<String, C1ArtifactError> {
    if bundle.authority_path != authority.authority_path
        || bundle.authority_sha256 != authority.authority_sha256
        || bundle.language_contract_version != authority.language_contract_version
    {
        return Err(C1ArtifactError::InvalidAuthority);
    }

    let d3_law = c_value_expr(&bundle.d3_law)?;
    let d4_law = c_value_expr(&bundle.d4_law)?;
    let request_provenance = c_value_expr(&bundle.request_provenance)?;
    let artifact_provenance = c_list(&[
        c_string(&authority.revision),
        c_string(bundle.authority_path),
        c_string(&bundle.authority_sha256),
        c_string(bundle.language_contract_version),
        c_string(&bundle.compiler_nucleus_sha256),
    ]);
    let args = c_list(&[
        "mk_builtin(\"domain-identity-shape-or-empty\", builtin_domain_identity_shape_or_empty)"
            .to_string(),
        "mk_builtin(\"domain-identity-shape\", builtin_domain_identity_shape)".to_string(),
        "mk_builtin(\"canonical-value-sha256\", builtin_canonical_value_sha256)".to_string(),
        "program".to_string(),
        d3_law,
        d4_law,
        c_string(bundle.d3_proof_ref),
        c_string(bundle.d4_proof_ref),
        request_provenance,
        artifact_provenance,
        "mk_string(program_wire_sha256)".to_string(),
    ]);

    Ok(format!(
        "    size_t c1_input_len = 0;\n\
         \x20   uint8_t *c1_input = c1_read_stdin_all(&c1_input_len);\n\
         \x20   char *program_wire_sha256 = c1_sha256_hex_bytes(c1_input, c1_input_len);\n\
         \x20   Value *program = decode_sens_program_wire(c1_input, c1_input_len);\n\
         \x20   Value *entry = env_lookup(global_env, \"COMPILER-COMPILE-PROGRAM-ARTIFACT\");\n\
         \x20   Value *artifact = v_apply(entry, {args});\n\
         \x20   c1_write_compiler_evidence(artifact);\n"
    ))
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
    if !valid_hex(&artifact.cml_revision, 40) {
        return Err(C1ArtifactError::InvalidCmlRevision);
    }
    for field in [
        &artifact.source,
        &artifact.source_sha256,
        &artifact.compiler_export,
        &artifact.compiler_export_sha256,
        &artifact.authority.repository,
        &artifact.authority.revision,
        &artifact.authority.authority_path,
        &artifact.authority.authority_sha256,
        &artifact.authority.language_contract_version,
        &artifact.cml_revision,
        &artifact.backend_id,
        &artifact.artifact_format,
        &artifact.c_source,
        &artifact.c_source_sha256,
    ] {
        if field.len() > MAX_FIELD_LEN {
            return Err(C1ArtifactError::FieldTooLarge);
        }
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
    cml_revision: &str,
) -> Result<CurrentSensC1Artifact, C1ArtifactError> {
    if source != PINNED_NUCLEUS {
        return Err(C1ArtifactError::WrongSourceBundle);
    }

    let lowered = lower_current_sens_source(source, compiler_export)?;
    let bundle = sens::compiler_program_bootstrap_bundle()
        .map_err(|error| C1ArtifactError::Bootstrap(error.to_string()))?;
    let driver_body = c1_driver_body(&lowered.authority, &bundle)?;
    let source_sha256 = sha256_hex(source.as_bytes());
    let mut backend = CBackend::new();
    let c_source = backend
        .compile_program_with_c1_driver(&lowered.ir, &driver_body)
        .map_err(|error| C1ArtifactError::Backend(error.to_string()))?;

    let artifact = CurrentSensC1Artifact {
        source: source.to_string(),
        source_sha256,
        compiler_export: compiler_export.to_string(),
        compiler_export_sha256: sha256_hex(compiler_export.as_bytes()),
        authority: lowered.authority,
        cml_revision: cml_revision.to_string(),
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
        push(&mut out, &self.source)?;
        push(&mut out, &self.source_sha256)?;
        push(&mut out, &self.compiler_export)?;
        push(&mut out, &self.compiler_export_sha256)?;
        push(&mut out, &self.authority.repository)?;
        push(&mut out, &self.authority.revision)?;
        push(&mut out, &self.authority.authority_path)?;
        push(&mut out, &self.authority.authority_sha256)?;
        push(&mut out, &self.authority.language_contract_version)?;
        push(&mut out, &self.cml_revision)?;
        push(&mut out, &self.backend_id)?;
        push(&mut out, &self.artifact_format)?;
        push(&mut out, &self.c_source)?;
        push(&mut out, &self.c_source_sha256)?;
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
            cml_revision: decoder.take_string()?,
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

fn push(out: &mut Vec<u8>, value: &str) -> Result<(), C1ArtifactError> {
    if value.len() > MAX_FIELD_LEN || value.len() > u32::MAX as usize {
        return Err(C1ArtifactError::FieldTooLarge);
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
        if len > MAX_FIELD_LEN {
            return Err(C1ArtifactError::FieldTooLarge);
        }
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
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::rc::Rc;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SOURCE: &str = include_str!("../external/sens/lib/compiler-nucleus.lisp");
    const FALLBACK_CML_REVISION: &str = "3fda3cdb087a24b3e124c811abff90803028518c";

    fn producer_cml_revision() -> String {
        std::env::var("CML_PRODUCER_SHA")
            .ok()
            .filter(|sha| valid_hex(sha, 40))
            .unwrap_or_else(|| FALLBACK_CML_REVISION.to_string())
    }

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

    fn expr_program_data(expr: &sens::Expr) -> sens::Value {
        use sens::ExprKind;
        match &expr.kind {
            ExprKind::Number(value, exactness) => sens::Value::Number(*value, *exactness),
            ExprKind::Rational(value) => sens::Value::Rational(value.clone()),
            ExprKind::BinaryNumber(value) => sens::Value::BinaryNumber(value.clone()),
            ExprKind::NumericBuffer(value) => sens::Value::NumericBuffer(value.clone()),
            ExprKind::DomainIdentity(identity) => sens::Value::DomainIdentity(*identity),
            ExprKind::String(value) => sens::Value::String(value.clone()),
            ExprKind::Symbol(value) => sens::Value::Symbol(value.clone()),
            ExprKind::List(items) => sens::Value::list(items.iter().map(expr_program_data)),
            ExprKind::Pair(head, tail) => sens::Value::Pair(
                Rc::new(expr_program_data(head)),
                Rc::new(expr_program_data(tail)),
            ),
            ExprKind::DomainCall(identity, arguments) => {
                let mut items = Vec::with_capacity(arguments.len() + 1);
                items.push(sens::Value::DomainIdentity((*identity).into()));
                items.extend(arguments.iter().map(expr_program_data));
                sens::Value::list(items)
            }
            ExprKind::Sid(_) | ExprKind::Call(_, _) => {
                panic!("current C1 program-data must not contain historical Sid/Call")
            }
            ExprKind::Local { .. } => {
                panic!("source-shaped C1 program-data must not contain resolved Local")
            }
        }
    }

    fn current_nucleus_program_wire() -> Vec<u8> {
        let parsed = sens::parse(SOURCE).expect("current compiler nucleus parses");
        let lowered = sens::lower_program(&parsed);
        sens::wire_encode_program(&lowered)
    }

    fn test_symbol(name: &str) -> sens::Expr {
        sens::Expr {
            kind: sens::ExprKind::Symbol(Rc::from(name)),
            span: sens::Span::default(),
        }
    }

    fn test_list(items: Vec<sens::Expr>) -> sens::Expr {
        sens::Expr {
            kind: sens::ExprKind::List(Rc::from(items.into_boxed_slice())),
            span: sens::Span::default(),
        }
    }

    fn d3(bits: u8) -> sens::CoreDomainIdentity {
        sens::CoreDomainIdentity::D3(sens::Bija3::from_word(
            sens::Bit3::new(bits).expect("D3 test identity"),
        ))
    }

    fn d4(bits: u8) -> sens::CoreDomainIdentity {
        sens::CoreDomainIdentity::D4(sens::CoreD4::from_word(
            sens::Bit4::new(bits).expect("D4 test identity"),
        ))
    }

    fn test_call(identity: sens::CoreDomainIdentity, args: Vec<sens::Expr>) -> sens::Expr {
        sens::Expr {
            kind: sens::ExprKind::DomainCall(identity, Rc::from(args.into_boxed_slice())),
            span: sens::Span::default(),
        }
    }

    fn minimal_current_programs() -> Vec<(&'static str, Vec<sens::Expr>)> {
        vec![
            ("quote", vec![test_call(d3(0b001), vec![test_symbol("x")])]),
            ("atom", vec![test_call(d3(0b010), vec![test_symbol("x")])]),
            ("cdr", vec![test_call(d3(0b011), vec![test_symbol("x")])]),
            ("car", vec![test_call(d3(0b100), vec![test_symbol("x")])]),
            (
                "eq",
                vec![test_call(
                    d3(0b101),
                    vec![test_symbol("x"), test_symbol("y")],
                )],
            ),
            (
                "cond",
                vec![test_call(
                    d3(0b110),
                    vec![test_list(vec![test_symbol("x"), test_symbol("y")])],
                )],
            ),
            (
                "cons",
                vec![test_call(
                    d3(0b111),
                    vec![test_symbol("x"), test_symbol("y")],
                )],
            ),
            (
                "lambda",
                vec![test_call(
                    d4(0b0010),
                    vec![test_list(vec![test_symbol("x")]), test_symbol("x")],
                )],
            ),
            (
                "define",
                vec![test_call(
                    d4(0b0011),
                    vec![test_symbol("f"), test_symbol("x")],
                )],
            ),
        ]
    }

    fn run_c1(binary_path: &std::path::Path, wire: &[u8]) -> std::process::Output {
        let mut child = Command::new(binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("C1 executable must start");
        child
            .stdin
            .take()
            .expect("C1 stdin pipe")
            .write_all(wire)
            .expect("program wire reaches C1");
        child.wait_with_output().expect("C1 executable must finish")
    }

    fn expected_sens_artifact(wire: &[u8], sens_revision: &str) -> Vec<u8> {
        let decoded = sens::wire_decode_program(wire).expect("canonical SW1 program wire");
        let program = sens::Value::list(decoded.iter().map(expr_program_data));
        let digest = sha256_hex(wire);
        let artifact = sens::compiler_program_artifact_from_sens(program, &digest, sens_revision)
            .expect("SENS whole-program artifact oracle");
        sens::compiler_evidence_canonical_bytes(&artifact)
            .expect("canonical compiler-evidence bytes")
    }

    #[test]
    fn c1_bundle_carries_source_export_proof_and_provenance() {
        let export = pinned_compiler_export();
        let cml_revision = producer_cml_revision();
        let artifact = build_current_sens_c1(SOURCE, export, &cml_revision)
            .expect("current nucleus C1 artifact");

        assert_eq!(artifact.source, SOURCE);
        assert_eq!(artifact.backend_id, C1_BACKEND_ID);
        assert_eq!(artifact.artifact_format, C1_ARTIFACT_FORMAT);
        assert!(valid_hex(&artifact.authority.revision, 40));
        assert_eq!(artifact.authority.repository, "juv4uk/sens");
        assert_eq!(artifact.cml_revision, cml_revision);
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
        assert!(artifact.c_source.contains("decode_sens_program_wire("));
        assert!(artifact.c_source.contains("builtin_canonical_value_sha256"));
        assert!(artifact.c_source.contains("c1_write_compiler_evidence"));
        assert!(!artifact.c_source.contains("D3_PROJECTION"));
        assert!(!artifact.c_source.contains("compiler_role_table"));
        assert!(
            artifact
                .c_source
                .contains("COMPILER-COMPILE-PROGRAM-ARTIFACT")
        );
        assert!(
            !artifact
                .c_source
                .contains("compiler_semantic_input_from_sens")
        );

        let decoded = CurrentSensC1Artifact::decode_v1(&artifact.encode_v1().unwrap()).unwrap();
        assert_eq!(decoded, artifact);
    }

    #[test]
    fn c1_source_compiles_and_emits_exact_sens_whole_program_artifact() {
        let cml_revision = producer_cml_revision();
        let artifact = build_current_sens_c1(SOURCE, pinned_compiler_export(), &cml_revision)
            .expect("current nucleus C1 artifact");
        let wire = current_nucleus_program_wire();
        let expected = expected_sens_artifact(&wire, &artifact.authority.revision);

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("cml-sens-c1-{}-{nonce}", std::process::id()));
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

        for (label, program) in minimal_current_programs() {
            let role_wire = sens::wire_encode_program(&program);
            let role_expected = expected_sens_artifact(&role_wire, &artifact.authority.revision);
            let role_run = run_c1(&binary_path, &role_wire);
            assert!(
                role_run.status.success(),
                "generated C1 failed minimal {label} program: {}",
                String::from_utf8_lossy(&role_run.stderr)
            );
            assert_eq!(
                role_run.stdout, role_expected,
                "compiled C1 diverged from SENS oracle on minimal {label} program"
            );
        }

        let run = run_c1(&binary_path, &wire);
        assert!(
            run.status.success(),
            "generated C1 executable failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            run.stdout, expected,
            "compiled C1 must emit byte-for-byte the SENS-owned canonical compiler evidence"
        );

        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(binary_path);
    }

    #[test]
    fn c1_recompiles_identical_nucleus_to_byte_identical_c2_artifact() {
        const SENS_DENOMINATOR: &str = "c66d4743bb70882c75376dbcec27d393e5a9649d";

        let cml_revision = producer_cml_revision();
        let c1 = build_current_sens_c1(SOURCE, pinned_compiler_export(), &cml_revision)
            .expect("merged current SENS nucleus must build as C1");
        assert_eq!(
            c1.authority.revision, SENS_DENOMINATOR,
            "C2 witness must run against the exact merged SENS denominator"
        );

        let wire = current_nucleus_program_wire();
        let c0_artifact = expected_sens_artifact(&wire, &c1.authority.revision);

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("cml-sens-c2-{}-{nonce}", std::process::id()));
        let source_path = base.with_extension("c");
        let binary_path = base.with_extension("bin");
        std::fs::write(&source_path, &c1.c_source).expect("write generated C1 source");

        let compile = Command::new("gcc")
            .arg(&source_path)
            .arg("-Wl,--build-id=none")
            .arg("-o")
            .arg(&binary_path)
            .output()
            .expect("gcc must execute");
        assert!(
            compile.status.success(),
            "generated C1 source did not compile for C2 witness: {}",
            String::from_utf8_lossy(&compile.stderr)
        );

        // C2 is the whole-program compiler artifact emitted by the generated
        // C1 process when that process compiles the identical compiler nucleus.
        // Run it twice so determinism is observed from C1, not inferred from C0.
        let first_c2 = run_c1(&binary_path, &wire);
        let second_c2 = run_c1(&binary_path, &wire);
        assert!(
            first_c2.status.success(),
            "C1 failed to produce C2: {}",
            String::from_utf8_lossy(&first_c2.stderr)
        );
        assert!(
            second_c2.status.success(),
            "C1 failed deterministic C2 repeat: {}",
            String::from_utf8_lossy(&second_c2.stderr)
        );
        assert_eq!(
            first_c2.stdout, second_c2.stdout,
            "identical C1 + nucleus + authority bundle must emit identical C2 bytes"
        );
        assert_eq!(
            first_c2.stdout, c0_artifact,
            "strongest current fixed-point witness is byte-identical C0/C1 compiler artifact output"
        );

        let c1_executable = std::fs::read(&binary_path).expect("read generated C1 executable");
        let nucleus_sha256 = sha256_hex(SOURCE.as_bytes());
        let c1_executable_sha256 = sha256_hex(&c1_executable);
        let c0_artifact_sha256 = sha256_hex(&c0_artifact);
        let c2_artifact_sha256 = sha256_hex(&first_c2.stdout);

        assert_eq!(
            c0_artifact_sha256, c2_artifact_sha256,
            "byte-identical criterion must not be silently downgraded"
        );

        println!(
            "SELFHOST_C2_EVIDENCE sens_revision={} cml_revision={} nucleus_sha256={} c1_c_source_sha256={} c1_executable_sha256={} c0_artifact_sha256={} c2_artifact_sha256={} equivalence=byte-identical repeat=byte-identical",
            c1.authority.revision,
            cml_revision,
            nucleus_sha256,
            c1.c_source_sha256,
            c1_executable_sha256,
            c0_artifact_sha256,
            c2_artifact_sha256,
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
            build_current_sens_c1(SOURCE, &tampered, &producer_cml_revision()).is_err(),
            "same payload under the wrong domain must fail before C1 artifact emission"
        );
    }

    #[test]
    fn modified_source_cannot_reuse_current_nucleus_proof_export() {
        let modified = format!("{SOURCE}\n; modified");
        assert_eq!(
            build_current_sens_c1(
                &modified,
                pinned_compiler_export(),
                &producer_cml_revision(),
            )
            .unwrap_err(),
            C1ArtifactError::WrongSourceBundle
        );
    }
}
