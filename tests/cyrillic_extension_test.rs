//! cml#4 (ecosystem-wide owner decision, 2026-09-10): Cyrillic-spelled file
//! extensions are equal, not translated -- `.wsm<->.всм`, `.my<->.мій`,
//! `.lisp<->.лісп`. Audit result for cml: `src/main.rs`'s file loading
//! (`fs::read_to_string`) and every parser/lowering/backend entry point
//! take source text, not a filename or extension -- nothing in cml ever
//! branches on file extension (verified: no `.extension()` call anywhere
//! in `src/`). So there is no dedicated "routing" to add for the compiler
//! itself; the only real extension-keyed surface was `.gitattributes`'s
//! GitHub Linguist classification, which this change extends.
//!
//! This test is the executable evidence the issue asks for: read each
//! Cyrillic-extensioned fixture from disk exactly like any other file,
//! run it through the same real `compile_and_run` pipeline a `.my`/`.wsm`/
//! `.lisp` file would use, and check it produces the expected value --
//! not just that the file exists.

use cml::build::{Observation, compile_and_run};
use std::fs;
use std::path::Path;

fn run_fixture(relative_path: &str) -> Observation {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    compile_and_run(&source).unwrap_or_else(|e| panic!("compile_and_run({relative_path}): {e}"))
}

#[test]
fn dot_mij_extension_compiles_and_runs_like_dot_my() {
    // tests/fixtures/приклад.мій: (+ 1 2)
    assert_eq!(
        run_fixture("tests/fixtures/приклад.мій"),
        Observation::Value("3".into())
    );
}

#[test]
fn dot_vsm_extension_compiles_and_runs_like_dot_wsm() {
    // tests/fixtures/знання.всм: (cons (quote знання) (quote сила))
    // Cyrillic quoted symbols round-trip through cml's uppercasing
    // target-symbol convention like any other symbol.
    match run_fixture("tests/fixtures/знання.всм") {
        Observation::Value(v) => assert_eq!(v, "(ЗНАННЯ . СИЛА)"),
        other => panic!("expected a cons pair, got {other:?}"),
    }
}

#[test]
fn dot_lisp_extension_compiles_and_runs_like_dot_lisp_latin() {
    // tests/fixtures/програма.лісп: (car (cons 42 (quote ())))
    assert_eq!(
        run_fixture("tests/fixtures/програма.лісп"),
        Observation::Value("42".into())
    );
}
