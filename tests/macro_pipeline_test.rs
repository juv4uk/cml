//! COMPILER-06 — macro expansion as an explicit compiler stage.
//!
//! Proves `defmacro` is collected and expanded before IR lowering and C
//! emission, using the same fixtures documented against `macros.lisp`
//! (my-list / my-if). The live in-process authority is Rust
//! `MacroExpander`; `macros.lisp` remains the parallel Lisp implementation.

use cml::ast::Expr;
use cml::build::{Observation, compile_and_run, expand_macros, parse_source};

fn value(source: &str) -> String {
    match compile_and_run(source).expect("compile_and_run") {
        Observation::Value(v) => v,
        other => panic!("expected Value, got {other:?} for {source}"),
    }
}

#[test]
fn expand_macros_is_explicit_stage() {
    let src =
        "(defmacro my-list items (cons (quote quote) (cons items (quote ())))) (my-list 1 2 3)";
    let exprs = parse_source(src).expect("parse");
    let expanded = expand_macros(&exprs).expect("expand");
    // defmacro form is consumed; remaining form is expanded to (quote (1 2 3))
    assert_eq!(expanded.len(), 1);
    match &expanded[0] {
        Expr::List(list) => {
            assert!(matches!(&list[0], Expr::Symbol(s) if s.eq_ignore_ascii_case("quote")));
        }
        other => panic!("expected list after expand, got {other:?}"),
    }
}

#[test]
fn defmacro_my_list_through_c_backend() {
    // compatibility.my / macros.lisp witness: (my-list 1 2 3) → (quote (1 2 3))
    // Evaluating quoted list via car of the expanded form is out of scope;
    // the expanded program is the quoted list itself as a value.
    let src = r#"(defmacro my-list items (cons (quote quote) (cons items (quote ()))))
(my-list 1 2 3)"#;
    let v = value(src);
    // Printed quoted list value: (1 2 3)
    assert_eq!(v, "(1 2 3)");
}

#[test]
fn defmacro_my_if_expands_to_cond_result() {
    let src = r#"(defmacro my-if (test then else)
  (cons (quote cond)
        (cons (cons test (cons then (quote ())))
              (cons (cons (quote t) (cons else (quote ()))) (quote ())))))
(my-if (eq 1 1) 42 0)"#;
    assert_eq!(value(src), "42");
}

#[test]
fn defmacro_my_if_else_branch() {
    let src = r#"(defmacro my-if (test then else)
  (cons (quote cond)
        (cons (cons test (cons then (quote ())))
              (cons (cons (quote t) (cons else (quote ()))) (quote ())))))
(my-if (eq 1 0) 1 99)"#;
    assert_eq!(value(src), "99");
}

#[test]
fn macro_body_unbound_symbol_is_error_not_skip() {
    let src = "(defmacro bad (x) no-such-meta-binding) (bad 1)";
    match compile_and_run(src).expect("compile_and_run") {
        Observation::Error(e) => {
            assert!(
                e.contains("Macro") || e.contains("unbound"),
                "expected Macro unbound error, got {e}"
            );
        }
        other => panic!("expected Error, got {other:?}"),
    }
}
