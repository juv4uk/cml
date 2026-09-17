#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use cml::c_backend::CBackend;
use cml::{lower, parser};

fn upstream_conformance_corpus() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp/tests/fixtures/conformance.lisp");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "#123 requires pinned upstream conformance corpus at {}: {error}",
            path.display()
        )
    })
}

fn alist_string_field(row: &str, key: &str) -> Option<String> {
    let marker = format!("({key} . \"");
    let tail = row.split_once(&marker)?.1;
    Some(tail.split_once("\")")?.0.to_string())
}

fn upstream_witness_by_source(corpus: &str, required_source: &str) -> (String, String) {
    corpus
        .lines()
        .find_map(|line| {
            let source = alist_string_field(line, "expr")?;
            if source != required_source {
                return None;
            }
            let expected = alist_string_field(line, "expected")?;
            Some((source, expected))
        })
        .unwrap_or_else(|| {
            panic!(
                "#123 requires pinned Lisp-owned exact-rational witness for source {required_source:?}"
            )
        })
}

fn gcc_command() -> Command {
    let mut cmd = Command::new("gcc");
    if std::env::var("C_INCLUDE_PATH").is_err()
        && std::path::Path::new("/var/guix/profiles/shared/guix-profile/include").exists()
    {
        cmd.env(
            "C_INCLUDE_PATH",
            "/var/guix/profiles/shared/guix-profile/include",
        );
    }
    cmd
}

fn compile_and_run_first_class(code: &str, stem: &str) -> String {
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    let c_source = CBackend::new().compile_program(&program).unwrap();
    let c_path = format!("c_backend_{stem}_test.c");
    let bin_path = format!("c_backend_{stem}_test");
    fs::write(&c_path, &c_source).unwrap();

    let compile = gcc_command()
        .arg(&c_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);
    assert!(
        run.status.success(),
        "compiled C program failed for upstream source {code:?}: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout).unwrap().trim().to_string()
}

#[test]
fn c_backend_exact_rational_classes_are_owned_by_upstream_lisp_witnesses() {
    let corpus = upstream_conformance_corpus();
    let cases = [
        ("add_reduction", "(+ (/ 1 3) (/ 1 3))"),
        ("sub_int_mix", "(- 1 (/ 1 3))"),
        ("mul_reduction", "(* (/ 2 3) (/ 9 4))"),
        ("div_rational_intermediate", "(/ 5 6 8 7)"),
        ("unary_minus", "(- (/ 1 3))"),
    ];

    for (stem, required_source) in cases {
        let (source, expected) = upstream_witness_by_source(&corpus, required_source);
        let actual = compile_and_run_first_class(&source, stem);
        assert_eq!(
            actual, expected,
            "CML must not own the exact-rational answer for upstream source {source:?}"
        );
    }
}
