//! COMPILER-01: source → IR → C → system C compiler → executable.
//!
//! Shared by the `cml build` CLI and the COMPILER-00 triple-oracle harness.
//! Failures are explicit Results — never silent skip.
//!
//! COMPILER-06: macro expansion is an explicit front-end stage
//! (`expand_macros`) before lowering. The Rust `MacroExpander` is the
//! live authority in-process; `macros.my` is the parallel Lisp
//! implementation (differential evidence only until a host embedding
//! decision wires it). 

use crate::ast::Expr;
use crate::c_backend::{self, CBackend};
use crate::ir::Ir;
use crate::lower::{self, LowerError};
use crate::macros::{MacroError, MacroExpander};
use crate::parser::{self, ParseError};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum BuildError {
    Io(String),
    Parse(ParseError),
    Macro(MacroError),
    Lower(LowerError),
    Emit(c_backend::CompileError),
    Toolchain(String),
    Run(String),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Io(msg) => write!(f, "Io: {msg}"),
            BuildError::Parse(err) => write!(f, "{err}"),
            BuildError::Macro(err) => write!(f, "Macro: {err}"),
            BuildError::Lower(err) => write!(f, "Lower: {err}"),
            BuildError::Emit(err) => write!(f, "Emit: {err}"),
            BuildError::Toolchain(msg) => write!(f, "Toolchain: {msg}"),
            BuildError::Run(msg) => write!(f, "Run: {msg}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// Structural classification used by the triple-oracle harness (COMPILER-00).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    Value(String),
    Error(String),
    Unsupported(String),
}

/// Options for `cml build`.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub output: PathBuf,
    pub keep_c: bool,
    pub c_path: Option<PathBuf>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            output: PathBuf::from("a.out"),
            keep_c: false,
            c_path: None,
        }
    }
}

/// COMPILER-06: explicit macro-expansion stage.
pub fn expand_macros(exprs: &[Expr]) -> Result<Vec<Expr>, BuildError> {
    MacroExpander::new()
        .process(exprs)
        .map_err(BuildError::Macro)
}

/// Parse only.
pub fn parse_source(source: &str) -> Result<Vec<Expr>, BuildError> {
    parser::parse(source).map_err(BuildError::Parse)
}

/// Parse + expand + lower (first-class builtins for C path).
pub fn front_end_to_ir(source: &str) -> Result<Vec<Ir>, BuildError> {
    let exprs = parse_source(source)?;
    let exprs = expand_macros(&exprs)?;
    lower::lower_program_with_first_class_builtins(&exprs).map_err(BuildError::Lower)
}

/// Emit C source for a lowered program.
pub fn emit_c(program: &[Ir]) -> Result<String, BuildError> {
    CBackend::new()
        .compile_program(program)
        .map_err(BuildError::Emit)
}

/// Compile C source with the system C compiler to `output`.
pub fn compile_c_to_executable(c_source: &str, output: &Path) -> Result<(), BuildError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!(
        "cml-build-{}-{nonce}.c",
        std::process::id()
    ));
    fs::write(&tmp, c_source).map_err(|e| BuildError::Io(e.to_string()))?;
    let cc = Command::new("cc")
        .arg(&tmp)
        .arg("-o")
        .arg(output)
        .arg("-O0")
        .output()
        .map_err(|e| BuildError::Toolchain(format!("failed to start cc: {e}")))?;
    let _ = fs::remove_file(&tmp);
    if !cc.status.success() {
        return Err(BuildError::Toolchain(format!(
            "cc failed:\n{}",
            String::from_utf8_lossy(&cc.stderr)
        )));
    }
    Ok(())
}

/// Full pipeline: source text → native executable at `opts.output`.
pub fn build_source(source: &str, opts: &BuildOptions) -> Result<(), BuildError> {
    let program = front_end_to_ir(source)?;
    let c_source = emit_c(&program)?;
    if let Some(c_path) = &opts.c_path {
        fs::write(c_path, &c_source).map_err(|e| BuildError::Io(e.to_string()))?;
    } else if opts.keep_c {
        let c_path = opts.output.with_extension("c");
        fs::write(&c_path, &c_source).map_err(|e| BuildError::Io(e.to_string()))?;
    }
    compile_c_to_executable(&c_source, &opts.output)
}

/// Build from a source file path.
pub fn build_file(path: &Path, opts: &BuildOptions) -> Result<(), BuildError> {
    let source =
        fs::read_to_string(path).map_err(|e| BuildError::Io(format!("{}: {e}", path.display())))?;
    build_source(&source, opts)
}

/// COMPILER-05: map runtime stderr "Kind: detail" to Observation::Error
/// with a stable kind prefix when recognized by Runtime ABI v0.
pub fn classify_runtime_stderr(stderr: &str) -> Observation {
    let line = stderr.lines().next().unwrap_or(stderr).trim();
    if line.is_empty() {
        return Observation::Error("exit failure".into());
    }
    for kind in crate::runtime_abi::ERROR_KINDS {
        let prefix = format!("{kind}:");
        if line.starts_with(&prefix) || line.starts_with(kind) {
            return Observation::Error(line.to_string());
        }
    }
    Observation::Error(line.to_string())
}

/// Compile source, run the executable, return stdout (trimmed) or structured error.
pub fn compile_and_run(source: &str) -> Result<Observation, BuildError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let bin = std::env::temp_dir().join(format!("cml-run-{}-{nonce}", std::process::id()));
    let opts = BuildOptions {
        output: bin.clone(),
        keep_c: false,
        c_path: None,
    };
    match build_source(source, &opts) {
        Ok(()) => {}
        Err(BuildError::Emit(e)) => {
            return Ok(Observation::Unsupported(format!("Emit: {e}")));
        }
        Err(BuildError::Lower(e)) => {
            let msg = e.to_string();
            if msg.contains("ReservedCanonName") || msg.contains("Unsupported") {
                return Ok(Observation::Unsupported(msg));
            }
            return Ok(Observation::Error(format!("Lower: {msg}")));
        }
        Err(BuildError::Macro(e)) => {
            return Ok(Observation::Error(format!("Macro: {e}")));
        }
        Err(BuildError::Parse(e)) => return Ok(Observation::Error(format!("{e}"))),
        Err(e) => return Err(e),
    }
    let run = Command::new(&bin)
        .output()
        .map_err(|e| BuildError::Run(format!("failed to start binary: {e}")))?;
    let _ = fs::remove_file(&bin);
    if run.status.success() {
        let out = String::from_utf8_lossy(&run.stdout).trim().to_string();
        Ok(Observation::Value(out))
    } else {
        let err = String::from_utf8_lossy(&run.stderr).trim().to_string();
        if err.is_empty() {
            Ok(Observation::Error(format!("exit {}", run.status)))
        } else {
            Ok(classify_runtime_stderr(&err))
        }
    }
}
