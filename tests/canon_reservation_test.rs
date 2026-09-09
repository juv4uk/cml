//! Contract 6.0 Canon-reservation static rejection tests for CML.
//!
//! These tests document the intentional break from the previous behaviour
//! where `(let ((car 1)) ...)` and `(def car 42)` were accepted. Under
//! my-lisp language contract 6.0 the finite Canon 0+7 surface set is
//! reserved: binders that attempt to bind those names must fail before
//! IR is emitted.
//!
//! CML still claims only contract 2.0 globally (see compatibility.my).
//! This is a targeted alignment of binder rejection with upstream.

use cml::lower::{self, LowerErrorKind};
use cml::parser;
use cml::semantic::{self, SemanticErrorKind};

fn must_reject_reserved(source: &str) {
    let expressions = parser::parse(source).expect("parse must succeed");
    let error = semantic::analyze_program(&expressions)
        .expect_err(&format!("expected ReservedCanonName for: {source}"));
    assert_eq!(
        error.kind,
        SemanticErrorKind::ReservedCanonName,
        "source: {source}, detail: {}",
        error.detail
    );
    assert!(
        error.detail.contains("canonical name is immutable")
            || error.detail.contains("канонічне"),
        "detail should mention immutability: {}",
        error.detail
    );

    // Lowering must also fail (semantic gate is invoked from lower_program).
    let lower_error = lower::lower_program(&expressions).expect_err("lower must fail");
    assert_eq!(lower_error.kind, LowerErrorKind::Semantic);
}

#[test]
fn rejects_def_of_latin_canon_names() {
    for source in [
        "(def car 42)",
        "(def cdr 42)",
        "(def cons 42)",
        "(def atom 42)",
        "(def eq 42)",
        "(def quote 42)",
        "(def cond 42)",
        "(def CAR 42)",
    ] {
        must_reject_reserved(source);
    }
}

#[test]
fn rejects_def_of_ukrainian_canon_names() {
    for source in [
        "(def перше 42)",
        "(def решта 42)",
        "(def як-є 42)",
        "(def атом? 42)",
        "(def тотожне? 42)",
        "(def сполучити 42)",
        "(def за-умовою 42)",
    ] {
        must_reject_reserved(source);
    }
}

#[test]
fn rejects_def_of_sanskrit_canon_names() {
    for source in [
        "(def ādi 42)",
        "(def śeṣa 42)",
        "(def svarūpa 42)",
        "(def aṇu 42)",
        "(def abheda 42)",
        "(def saṃyuj 42)",
        "(def anukrama 42)",
    ] {
        must_reject_reserved(source);
    }
}

#[test]
fn rejects_lambda_parameter_canon_names() {
    for source in [
        "(lambda (car) car)",
        "(lambda (перше) перше)",
        "(lambda (ādi) ādi)",
        "(lambda (x car) x)",
        "(lambda car car)", // bare rest parameter
    ] {
        must_reject_reserved(source);
    }
}

#[test]
fn rejects_let_binding_canon_names() {
    for source in [
        "(let ((car 1)) car)",
        "(let ((перше 1)) перше)",
        "(let ((ādi 1)) ādi)",
        "(let ((x 1) (cdr 2)) x)",
    ] {
        must_reject_reserved(source);
    }
}

#[test]
fn still_allows_ordinary_bindings() {
    let sources = [
        "(def length 42)",
        "(def map 42)",
        "(lambda (x) x)",
        "(lambda (f xs) xs)",
        "(let ((x 1)) x)",
        "(let ((f car)) f)", // binding *to* a Canon value is fine; name is ordinary
    ];
    for source in sources {
        let expressions = parser::parse(source).expect(source);
        semantic::analyze_program(&expressions)
            .unwrap_or_else(|e| panic!("should accept `{source}`: {e}"));
        lower::lower_program(&expressions)
            .unwrap_or_else(|e| panic!("should lower `{source}`: {e}"));
    }
}

#[test]
fn non_canon_builtins_remain_shadowable() {
    // `+` is not Canon 0+7; lexical shadowing remains allowed (Contract 6.0).
    let source = "(let ((+ 1)) +)";
    let expressions = parser::parse(source).unwrap();
    semantic::analyze_program(&expressions).unwrap();
    lower::lower_program(&expressions).unwrap();
}

#[test]
fn quoted_canon_shaped_data_is_not_treated_as_binding() {
    // Data, not a binder — must remain accepted.
    let source = "(quote (def car 42))";
    let expressions = parser::parse(source).unwrap();
    semantic::analyze_program(&expressions).unwrap();
}
