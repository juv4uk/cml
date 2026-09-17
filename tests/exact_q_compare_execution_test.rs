use std::fs;
use std::process::Command;

use cml::c_backend::CBackend;
use cml::{lower, parser};

fn field<'a>(record: &'a str, name: &str) -> &'a str {
    let prefix = format!("({name} . \"");
    record
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix("\")"))
        })
        .unwrap_or_else(|| panic!("upstream exact-Q fixture record has no {name} field"))
}

#[test]
fn c_backend_executes_upstream_exact_q_le_witness_without_host_truthiness() {
    // Semantic expected truth stays upstream. This test selects the current
    // Lisp-owned 1017 witness and compares only CML's actual compiled output.
    let fixture = fs::read_to_string("external/my-lisp/tests/fixtures/exact-q-binary-v1.lisp")
        .expect("vendored my-lisp exact-Q witness corpus must be present");
    let record = fixture
        .split("\n\n")
        .find(|record| record.contains("(identity . \"1017\")"))
        .expect("upstream exact-Q corpus must contain semantic identity 1017");
    let source = field(record, "expr");
    let expected = field(record, "expected");

    let expressions = parser::parse(source).expect("upstream exact-Q witness must parse");
    let program = lower::lower_program_with_first_class_builtins(&expressions)
        .expect("admitted exact-Q witness must lower for the C backend");
    let c_source = CBackend::new()
        .compile_program(&program)
        .expect("C backend must compile admitted exact-Q comparison IR");

    let nonce = std::process::id();
    let c_path = format!("c_backend_exact_q_le_{nonce}.c");
    let bin_path = format!("c_backend_exact_q_le_{nonce}");
    fs::write(&c_path, &c_source).unwrap();

    let compile = Command::new("gcc")
        .arg(&c_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "gcc failed: {}\n--- generated C ---\n{c_source}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);
    assert!(
        run.status.success(),
        "compiled exact-Q witness failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), expected);
}
