use std::fs;
use std::process::Command;

use cml::c_backend::CBackend;
use cml::ir::Ir;
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
fn exact_q_le_reaches_a_distinct_numeric_comparison_ir() {
    let expressions = parser::parse("(<= 128 191)").expect("exact-Q <= witness must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("admitted exact-Q <= witness must reach structural lowering");
    let [node] = lowered.as_slice() else {
        panic!("expected one lowered exact-Q comparison, got {lowered:?}");
    };

    match node {
        Ir::Prim { op, args } => {
            // #92 requires a distinct exact-Q comparison mechanism. Numeric
            // 1017 must not reuse atom identity 0003 (`PrimOp::Eq`) and must
            // not stay a generic App. The exact variant spelling is a local
            // compiler detail; this first slice names it explicitly so the
            // RED cannot go green through metadata-only admission.
            assert_eq!(format!("{op:?}"), "ExactQLe");
            assert_eq!(args, &[Ir::Int(128), Ir::Int(191)]);
        }
        Ir::App { .. } => panic!(
            "semantic 1017 is admitted but still lowers as generic App; #92 requires a distinct exact-Q comparison IR"
        ),
        other => panic!("semantic 1017 must lower as exact-Q comparison IR, got {other:?}"),
    }
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
