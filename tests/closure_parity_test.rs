//! COMPILER-04 — closure / binding parity on the C backend path.
//!
//! Proves fixed arity, lexical capture, first-class lambda arguments,
//! variadic/rest params, and self-recursion through `cml::build::compile_and_run`.
//! Mutual top-level recursion works via two-pass placeholder installation
//! (COMPILER-12): all top-level names are visible before any closure captures
//! the global_env list pointer.

use cml::build::{compile_and_run, Observation};

fn value(source: &str) -> String {
    match compile_and_run(source).expect("compile_and_run") {
        Observation::Value(v) => v,
        other => panic!("expected Value, got {other:?} for {source}"),
    }
}

fn error_kind(source: &str) -> String {
    match compile_and_run(source).expect("compile_and_run") {
        Observation::Error(e) => e,
        other => panic!("expected Error, got {other:?} for {source}"),
    }
}

#[test]
fn fixed_arity_lambda_application() {
    assert_eq!(value("((lambda (x) (+ x 1)) 41)"), "42");
}

#[test]
fn fixed_arity_two_params() {
    assert_eq!(value("((lambda (a b) (+ a b)) 10 32)"), "42");
}

#[test]
fn lexical_capture_from_outer_lambda() {
    assert_eq!(
        value("(((lambda (x) (lambda (y) (+ x y))) 10) 32)"),
        "42"
    );
}

#[test]
fn first_class_lambda_as_argument() {
    assert_eq!(
        value("((lambda (f x) (f x)) (lambda (n) (+ n 1)) 41)"),
        "42"
    );
}

#[test]
fn self_recursive_def_via_letrec_placeholder() {
    assert_eq!(
        value(
            "(def count (lambda (n) (cond ((eq n 0) 0) (t (+ 1 (count (- n 1))))))) (count 5)"
        ),
        "5"
    );
}

#[test]
fn variadic_rest_param() {
    assert_eq!(
        value("((lambda (a . rest) (car rest)) 1 2 3)"),
        "2"
    );
}

#[test]
fn all_rest_param() {
    assert_eq!(
        value("((lambda args (car args)) 7 8 9)"),
        "7"
    );
}

#[test]
fn lambda_arity_mismatch_is_named_error() {
    let err = error_kind("((lambda (x y) (+ x y)) 1)");
    assert!(
        err.contains("Arity"),
        "expected Arity kind, got {err}"
    );
}

#[test]
fn nested_let_acts_as_lambda_binding() {
    assert_eq!(
        value("(let ((f (lambda (x) (+ x x)))) (f 21))"),
        "42"
    );
}

/// COMPILER-04/12: two-pass top-level defs install all placeholders first,
/// so mutual recursion resolves through the shared global_env chain.
#[test]
fn mutual_recursion_even_odd() {
    let source = r#"(def even (lambda (n) (cond ((eq n 0) t) (t (odd (- n 1))))))
(def odd (lambda (n) (cond ((eq n 0) ()) (t (even (- n 1))))))
(even 4)"#;
    let v = match compile_and_run(source).expect("compile_and_run") {
        Observation::Value(v) => v,
        other => panic!("expected Value for mutual recursion, got {other:?}"),
    };
    assert!(v == "T" || v == "t", "expected canonical t, got {v}");
}
