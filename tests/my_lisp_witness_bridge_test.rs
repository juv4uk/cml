//! #116 cross-repo witness bridge.
//!
//! CML consumes the committed my-lisp conformance corpus as input. This
//! adapter deliberately does not contain an expected value: semantic truth
//! stays in my-lisp's Lisp-authored witness row.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::path::PathBuf;

use cml::witness_bridge::execute_x86_actual;
use cml::x86_freestanding::X86FreestandingBackend;
use cml::{lower, parser};
use my_lisp::{eval_program, load_core_library, Session};

fn upstream_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp")
        .join(relative)
}

fn upstream_corpus() -> String {
    let path = upstream_path("tests/fixtures/conformance.lisp");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "#116 requires the external/my-lisp submodule's conformance corpus at {}: {error}",
            path.display()
        )
    })
}

fn first_compiler_witness_expr(corpus: &str) -> String {
    corpus
        .lines()
        .filter(|line| !line.trim_start().starts_with(';'))
        .find(|line| line.contains("(compiler-corpus . t)"))
        .and_then(|line| line.split_once("((expr . \"").map(|(_, tail)| tail))
        .and_then(|tail| tail.split_once("\")").map(|(expr, _)| expr.to_string()))
        .expect("#116 requires at least one compiler-corpus witness with an expr")
}

fn compiler_witness_row<'a>(corpus: &'a str, expr: &str) -> &'a str {
    let marker = format!("((expr . \"{expr}\")");
    corpus
        .lines()
        .filter(|line| !line.trim_start().starts_with(';'))
        .find(|line| line.contains(&marker) && line.contains("(compiler-corpus . t)"))
        .unwrap_or_else(|| panic!("#46 requires compiler-corpus witness {expr}"))
}

fn lisp_owned_verdict_passes(row: &str, actual: &str) {
    let runner_path = upstream_path("tests/fixtures/witness-runner.lisp");
    let runner = fs::read_to_string(&runner_path).unwrap_or_else(|error| {
        panic!(
            "#46 requires Lisp-owned witness verdict protocol at {}: {error}",
            runner_path.display()
        )
    });

    let mut session = Session::default();
    load_core_library(&mut session).expect("pinned my-lisp core library must load");
    eval_program(&runner, &mut session).expect("pinned witness-runner.lisp must load");

    let verdict = eval_program(
        &format!(
            "(witness-pass? (witness-verdict (quote {row}) (quote {actual})))"
        ),
        &mut session,
    )
    .expect("Lisp-owned witness verdict must execute")
    .value
    .to_string();

    assert_eq!(
        verdict, "t",
        "semantic PASS must come from Lisp-owned witness-verdict, actual={actual}"
    );
}

#[test]
fn cml_consumes_upstream_lisp_witness_without_own_expected_answer() {
    let corpus = upstream_corpus();
    let source = first_compiler_witness_expr(&corpus);

    let expressions = parser::parse(&source)
        .unwrap_or_else(|error| panic!("CML could not parse upstream witness {source}: {error:?}"));
    let program = lower::lower_program(&expressions)
        .unwrap_or_else(|error| panic!("CML could not lower upstream witness {source}: {error}"));
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .unwrap_or_else(|error| {
            panic!("CML could not compile upstream witness {source}: {error:?}")
        });

    assert!(
        assembly.contains(".globl wsm_entry"),
        "upstream witness must reach the real x86 freestanding backend"
    );
}

#[test]
fn executed_cml_actual_is_judged_only_by_lisp_owned_witness_logic() {
    // Choose a committed compiler-corpus row by source expression only.
    // Its expected outcome is never copied into this Rust test.
    let corpus = upstream_corpus();
    let source = "(atom (quote radio))";
    let row = compiler_witness_row(&corpus, source);

    let expressions =
        parser::parse(source).expect("selected upstream compiler witness must parse in CML");
    let program = lower::lower_program(&expressions)
        .expect("selected upstream compiler witness must lower in CML");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("selected upstream compiler witness must compile to freestanding x86");

    let actual = execute_x86_actual(&assembly)
        .expect("#46 requires executing CML output and exporting canonical actual outcome");

    lisp_owned_verdict_passes(row, &actual);
}
