//! RED external diagnostic contract for #132.

use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn cml_compile_reports_machine_readable_parse_location() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("cml-diagnostic-span-{nonce}"));
    fs::create_dir_all(&root).expect("temp dir");
    let source = root.join("broken.lisp");
    let output = root.join("broken.elf");
    fs::write(&source, "(quote ok)\n\n)\n").expect("write fixture");

    let result = Command::new(env!("CARGO_BIN_EXE_cml-compile"))
        .arg("x86-elf")
        .arg(&source)
        .arg(&output)
        .output()
        .expect("run cml-compile");

    let _ = fs::remove_dir_all(&root);

    assert!(
        !result.status.success(),
        "malformed source must fail closed"
    );
    let stderr = String::from_utf8(result.stderr).expect("stderr utf8");
    assert!(
        stderr.lines().any(|line| {
            line.starts_with("CML-DIAGNOSTIC\t")
                && line.contains("\tstage=parse\t")
                && line.contains("\tline=3\t")
                && line.contains("\tcolumn=1\t")
        }),
        "expected stable parse diagnostic with line/column, got {stderr:?}"
    );
}
