// #140 acceptance: execution parity for the semantic-1007 mod slice the
// whole-encoder proof needs. The upstream encoder-derived helper is
//
//   (def x86-low3 (lambda (code) (mod code 8)))
//
// and the pinned upstream rows, verified live against the my-lisp evaluator
// at the current submodule, are low3(0)=0, low3(7)=7, low3(8)=0, low3(15)=7.
// This test compiles the REAL named function (dynamic numerator in a lambda
// body) through X86FreestandingBackend, assembles an ELF, runs it, and
// compares the decoded target fixnum against those upstream rows.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::x86_freestanding::X86FreestandingBackend;
use cml::{lower, parser};

fn run_low3(numerator: i64) -> i64 {
    let source = format!("(def x86-low3 (lambda (code) (mod code 8)))\n(x86-low3 {numerator})");
    let expressions = parser::parse(&source).expect("low3 fixture must parse");
    let program = lower::lower_program(&expressions).expect("low3 fixture must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("semantic-1007 mod inside a named function must compile on x86");

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock must be after epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-low3-parity-{}-{nonce}", std::process::id()));
    let asm_path = base.with_extension("s");
    let c_path = base.with_extension("c");
    let exe_path = base.with_extension("bin");

    fs::write(&asm_path, assembly).expect("write generated assembly");
    fs::write(
        &c_path,
        "#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n\nextern uint64_t wsm_entry(void *);\nvoid wsm_fail(void *ctx, unsigned code, uint64_t a, uint64_t b) {\n    (void)ctx; (void)code; (void)a; (void)b; abort();\n}\n\nint main(void) {\n    uint64_t r = wsm_entry(0);\n    printf(\"%llu\\n\", (unsigned long long)r);\n    return 0;\n}\n",
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
        "low3 witness must link: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = Command::new(&exe_path)
        .output()
        .expect("compiled low3 witness must execute");

    let _ = fs::remove_file(&asm_path);
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(&exe_path);

    assert!(
        run.status.success(),
        "low3 must run cleanly for {numerator}"
    );
    let stdout = String::from_utf8(run.stdout).expect("stdout must be valid UTF-8");
    let tagged: u64 = stdout
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("witness must print one tagged word, got {stdout:?}"));
    wsm_os_target::decode_fixnum(tagged).expect("low3 result must remain an exact target fixnum")
}

#[test]
fn x86_low3_parity_matches_upstream_rows_for_encoder_fixnum_domain() {
    // Rows verified live against the upstream my-lisp evaluator at the
    // current pinned submodule (e.g. `(mod 15 8)` -> 7).
    for (numerator, expected) in [(0, 0), (7, 7), (8, 0), (15, 7)] {
        assert_eq!(
            run_low3(numerator),
            expected,
            "semantic-1007 low3 parity row low3({numerator})"
        );
    }
}
