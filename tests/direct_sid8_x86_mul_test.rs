#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::x86_freestanding::X86FreestandingBackend;
use cml::{lower, parser};

fn compile_and_run(source: &str, stem: &str) -> (std::process::ExitStatus, Vec<u8>) {
    let expressions = parser::parse(source).expect("source must parse");
    let program = lower::lower_program(&expressions).expect("source must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("exact Sid8 multiplication must compile");

    assert!(
        assembly.contains("imulq"),
        "multiplication must use native imulq"
    );
    assert!(
        assembly.contains("jo .Larith_overflow_"),
        "multiplication must keep overflow guard"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-{stem}-{}-{nonce}", std::process::id()));
    let asm = base.with_extension("s");
    let harness = base.with_extension("c");
    let exe = base.with_extension("bin");

    fs::write(&asm, assembly).unwrap();
    fs::write(
        &harness,
        "#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n\nextern uint64_t wsm_entry(void *);\nvoid wsm_fail(void *ctx, unsigned code, uint64_t a, uint64_t b) { (void)ctx; (void)code; (void)a; (void)b; abort(); }\nint main(void) { printf(\"%llu\\n\", (unsigned long long)wsm_entry(0)); return 0; }\n",
    )
    .unwrap();

    let linked = Command::new("cc")
        .arg(&harness)
        .arg(&asm)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("cc must be available");
    assert!(
        linked.status.success(),
        "link failed: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let output = Command::new(&exe)
        .output()
        .expect("native witness must execute");
    let _ = fs::remove_file(asm);
    let _ = fs::remove_file(harness);
    let _ = fs::remove_file(exe);
    (output.status, output.stdout)
}

#[test]
fn exact_sid8_multiplication_executes_natively() {
    let (status, stdout) = compile_and_run("(* 3 7)", "sid8-mul-ok");
    assert!(status.success());
    let tagged: u64 = String::from_utf8(stdout).unwrap().trim().parse().unwrap();
    assert_eq!(wsm_os_target::decode_fixnum(tagged), Some(21));
}

#[test]
fn exact_sid8_multiplication_overflow_fails_closed() {
    let source = format!("(* {} 2)", wsm_os_target::FIXNUM_MAX);
    let (status, _) = compile_and_run(&source, "sid8-mul-overflow");
    assert!(!status.success(), "overflow must enter wsm_fail");
}
