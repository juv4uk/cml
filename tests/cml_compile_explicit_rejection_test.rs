use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn cml_compile_rejects_unsupported_source_without_leaving_an_artifact() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-107-cli-reject-{nonce}"));
    let source = base.with_extension("lisp");
    let output = base.with_extension("elf");

    fs::write(&source, "0.5\n").expect("write exact-rational source");
    assert!(!output.exists(), "test output path must start absent");

    let result = Command::new(env!("CARGO_BIN_EXE_cml-compile"))
        .arg("x86-elf")
        .arg(&source)
        .arg(&output)
        .output()
        .expect("launch cml-compile");

    let _ = fs::remove_file(&source);

    assert!(
        !result.status.success(),
        "unsupported target representation must return non-zero"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("Rational"),
        "rejection must preserve a named target reason, got: {stderr}"
    );
    assert!(
        !output.exists(),
        "failed compilation must not leave a nominally successful artifact"
    );
}
