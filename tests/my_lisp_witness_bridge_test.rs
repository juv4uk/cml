//! #116 cross-repo witness bridge.
//!
//! CML consumes the committed my-lisp conformance corpus as input.  This
//! adapter deliberately does not contain an expected value: semantic truth
//! stays in my-lisp's Lisp-authored witness row.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::path::PathBuf;

use cml::{lower, parser};
use cml::x86_freestanding::X86FreestandingBackend;

fn upstream_corpus() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../my-lisp/tests/fixtures/conformance.lisp");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("#116 requires the sibling my-lisp conformance corpus at {}: {error}", path.display())
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

#[test]
fn cml_consumes_upstream_lisp_witness_without_own_expected_answer() {
    let corpus = upstream_corpus();
    let source = first_compiler_witness_expr(&corpus);

    let expressions = parser::parse(&source)
        .unwrap_or_else(|error| panic!("CML could not parse upstream witness `{source}`: {error:?}"));
    let program = lower::lower_program(&expressions)
        .unwrap_or_else(|error| panic!("CML could not lower upstream witness `{source}`: {error}"));
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .unwrap_or_else(|error| panic!("CML could not compile upstream witness `{source}`: {error:?}"));

    assert!(assembly.contains(".globl wsm_entry"), "upstream witness must reach the real x86 freestanding backend");
}
