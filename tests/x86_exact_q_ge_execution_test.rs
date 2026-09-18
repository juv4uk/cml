use cml::{lower, parser, x86_freestanding::X86FreestandingBackend};
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn assert_x86_ge(source: &str, expected_numeric: i64) {
    let expressions = parser::parse(source).expect("ExactQGe fixture must parse");
    let program = lower::lower_program(&expressions).expect("ExactQGe fixture must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("semantic 1018 ExactQGe must compile on the x86 backend");

    let expected =
        wsm_os_target::encode_fixnum(expected_numeric).expect("0/1 are target fixnums");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock must be after epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-x86-exact-q-ge-{}-{nonce}",
        std::process::id()
    ));
    let asm_path = base.with_extension("s");
    let c_path = base.with_extension("c");
    let exe_path = base.with_extension("bin");

    fs::write(&asm_path, assembly).expect("write generated assembly");
    fs::write(
        &c_path,
        format!(
            "#include <stdint.h>\n#include <stdlib.h>\n\nextern uint64_t wsm_entry(void *);\nvoid wsm_fail(void *ctx, unsigned code, uint64_t a, uint64_t b) {{\n    (void)ctx; (void)code; (void)a; (void)b; abort();\n}}\n\nint main(void) {{\n    return wsm_entry(0) == UINT64_C({expected}) ? 0 : 1;\n}}\n"
        ),
    )
    .expect("write C witness harness");

    let linked = Command::new("cc")
        .arg(&c_path)
        .arg(&asm_path)
        .arg("-o")
        .arg(&exe_path)
        .output()
        .expect("cc must be available for x86 witness");
    assert!(
        linked.status.success(),
        "ExactQGe witness must link: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = Command::new(&exe_path)
        .output()
        .expect("compiled ExactQGe witness must execute");

    let _ = fs::remove_file(&asm_path);
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(&exe_path);

    assert!(
        run.status.success(),
        "ExactQGe must return exact numeric {expected_numeric} for {source}"
    );
}

#[test]
fn x86_exact_q_ge_returns_numeric_one_and_zero() {
    assert_x86_ge("(>= 194 128)", 1);
    assert_x86_ge("(>= 127 128)", 0);
}
