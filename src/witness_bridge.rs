use std::fmt;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum WitnessBridgeError {
    Io(String),
    Link(String),
    Execute(String),
    InvalidOutput(String),
    UnsupportedActual(wsm_os_target::Word),
}

impl fmt::Display for WitnessBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "witness bridge I/O error: {message}"),
            Self::Link(message) => write!(formatter, "witness bridge link error: {message}"),
            Self::Execute(message) => {
                write!(formatter, "witness bridge execution error: {message}")
            }
            Self::InvalidOutput(message) => {
                write!(
                    formatter,
                    "witness bridge invalid execution output: {message}"
                )
            }
            Self::UnsupportedActual(word) => {
                write!(
                    formatter,
                    "witness bridge cannot canonicalize target word {word:#x}"
                )
            }
        }
    }
}

impl std::error::Error for WitnessBridgeError {}

pub fn canonical_actual_from_word(word: wsm_os_target::Word) -> Result<String, WitnessBridgeError> {
    let rendered = if word == wsm_os_target::NIL {
        "()".to_string()
    } else if word == wsm_os_target::CANONICAL_T {
        "t".to_string()
    } else if let Some(value) = wsm_os_target::decode_fixnum(word) {
        value.to_string()
    } else {
        return Err(WitnessBridgeError::UnsupportedActual(word));
    };

    Ok(format!("(value \"{rendered}\")"))
}

pub fn execute_x86_actual(assembly: &str) -> Result<String, WitnessBridgeError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| WitnessBridgeError::Io(error.to_string()))?
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-witness-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let launcher = base.with_extension("c");
    let executable = base.with_extension("bin");

    fs::write(&source, assembly).map_err(|error| WitnessBridgeError::Io(error.to_string()))?;
    fs::write(
        &launcher,
        "#include <stdint.h>\n#include <stdio.h>\nextern uint64_t wsm_entry(void *);\nint main(void) { printf(\"%llu\\n\", (unsigned long long)wsm_entry(0)); return 0; }\n",
    )
    .map_err(|error| WitnessBridgeError::Io(error.to_string()))?;

    let nucleus =
        crate::x86_freestanding::resolve_nucleus_asm_path().map_err(WitnessBridgeError::Link)?;
    let linked = Command::new("cc")
        .arg(&launcher)
        .arg(&source)
        .arg(&nucleus)
        .arg("-o")
        .arg(&executable)
        .output()
        .map_err(|error| WitnessBridgeError::Link(error.to_string()))?;

    let _ = fs::remove_file(&source);
    let _ = fs::remove_file(&launcher);

    if !linked.status.success() {
        let _ = fs::remove_file(&executable);
        return Err(WitnessBridgeError::Link(
            String::from_utf8_lossy(&linked.stderr).into_owned(),
        ));
    }

    let output = Command::new(&executable)
        .output()
        .map_err(|error| WitnessBridgeError::Execute(error.to_string()))?;
    let _ = fs::remove_file(&executable);

    if !output.status.success() {
        return Err(WitnessBridgeError::Execute(format!(
            "exit status {}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| WitnessBridgeError::InvalidOutput(error.to_string()))?;
    let word = stdout
        .trim()
        .parse::<wsm_os_target::Word>()
        .map_err(|error| WitnessBridgeError::InvalidOutput(error.to_string()))?;

    canonical_actual_from_word(word)
}
