use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn profile_command_emits_phase_rows_without_changing_the_artifact_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_path = manifest_dir.join("tests/fixtures/vertical_witness_add.lisp");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output_path = std::env::temp_dir().join(format!("cml-profile-{nonce}"));

    let output = Command::new(env!("CARGO_BIN_EXE_cml-compile"))
        .arg("x86-elf-profile")
        .arg(&source_path)
        .arg(&output_path)
        .output()
        .expect("launch profiled compiler");

    assert!(
        output.status.success(),
        "profiled compiler failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let artifact = fs::read(&output_path).expect("profiled compile must emit an artifact");
    let _ = fs::remove_file(&output_path);
    assert!(artifact.starts_with(b"\x7fELF"));

    let stderr = String::from_utf8(output.stderr).expect("phase output must be UTF-8");
    let mut names = Vec::new();
    for line in stderr.lines().filter(|line| line.starts_with("CML-PHASE\t")) {
        let mut name = None;
        let mut elapsed = None;
        for field in line.split('\t').skip(1) {
            if let Some(value) = field.strip_prefix("name=") {
                name = Some(value);
            } else if let Some(value) = field.strip_prefix("elapsed_ns=") {
                elapsed = Some(
                    value
                        .parse::<u128>()
                        .expect("elapsed_ns must be an unsigned integer"),
                );
            }
        }
        names.push(name.expect("phase row must name the phase"));
        assert!(elapsed.is_some(), "phase row must carry elapsed_ns");
    }

    assert_eq!(
        names,
        vec![
            "load",
            "parse",
            "macro-expand",
            "lower",
            "target-codegen",
            "emit",
            "total",
        ]
    );
}
