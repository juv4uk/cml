use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn cml_compile_x86_elf_accepts_supported_multi_form_programs() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-compile-multiform-{nonce}"));
    let source_path = base.with_extension("lisp");
    let output_path = base.with_extension("elf");

    fs::write(&source_path, "(def id (lambda (x) x))\n(id 42)\n")
        .expect("write multi-form Lisp fixture");

    let status = Command::new(env!("CARGO_BIN_EXE_cml-compile"))
        .arg("x86-elf")
        .arg(&source_path)
        .arg(&output_path)
        .status()
        .expect("launch cml-compile");

    assert!(
        status.success(),
        "external cml-compile must route supported multi-form programs through the admitted freestanding backend"
    );
    assert!(
        output_path.exists(),
        "external compiler must produce the requested ELF artifact"
    );

    let executed = Command::new(&output_path)
        .status()
        .expect("execute linked multi-form ELF");
    assert!(
        executed.success(),
        "linked multi-form ELF must execute through the existing launcher boundary"
    );

    let _ = fs::remove_file(source_path);
    let _ = fs::remove_file(output_path);
}
