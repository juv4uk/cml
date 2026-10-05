use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_path(prefix: &str, nonce: u128) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{nonce}"))
}

#[test]
fn profile_command_emits_phase_rows_without_changing_the_artifact() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_path = manifest_dir.join("tests/fixtures/vertical_witness_add.lisp");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let normal_path = temp_path("cml-normal", nonce);
    let profile_path = temp_path("cml-profile", nonce);

    let normal = Command::new(env!("CARGO_BIN_EXE_cml-compile"))
        .arg("x86-elf")
        .arg(&source_path)
        .arg(&normal_path)
        .output()
        .expect("launch ordinary compiler");
    assert!(
        normal.status.success(),
        "ordinary compiler failed: {}",
        String::from_utf8_lossy(&normal.stderr)
    );

    let profiled = Command::new(env!("CARGO_BIN_EXE_cml-compile"))
        .arg("x86-elf-profile")
        .arg(&source_path)
        .arg(&profile_path)
        .output()
        .expect("launch profiled compiler");
    assert!(
        profiled.status.success(),
        "profiled compiler failed: {}",
        String::from_utf8_lossy(&profiled.stderr)
    );

    let normal_artifact = fs::read(&normal_path).expect("ordinary compile must emit an artifact");
    let profiled_artifact =
        fs::read(&profile_path).expect("profiled compile must emit an artifact");
    let _ = fs::remove_file(&normal_path);
    let _ = fs::remove_file(&profile_path);

    assert!(profiled_artifact.starts_with(b"\x7fELF"));
    assert_eq!(
        profiled_artifact, normal_artifact,
        "observational profiling must not change emitted bytes"
    );

    let stderr = String::from_utf8(profiled.stderr).expect("phase output must be UTF-8");
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
