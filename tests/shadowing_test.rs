//! Updated shadowing tests after Contract 6.0 Canon reservation.
//!
//! Previous behaviour allowed `(let ((car 1)) ...)` to shadow the Canon
//! primitive. Contract 6.0 makes all Canon 0+7 surfaces reserved and
//! unshadowable. These tests now assert that rejection.
//!
//! Non-Canon builtins (`+`, etc.) and ordinary names remain shadowable.
//! Binding a variable *to* a Canon value (e.g. `(let ((f car)) ...)`) is
//! still valid — only the *name* of the binder is restricted.

use cml::ast::Expr;
use cml::ir::Ir;
use cml::lower::lower_expr;
use cml::parser;
use cml::semantic::{self, SemanticErrorKind};

#[test]
fn canon_name_cannot_be_let_bound() {
    // (let ((car 1)) (car '(1 2)))  — must reject at semantic gate
    let source = "(let ((car 1)) (car (quote (1 2))))";
    let expressions = parser::parse(source).unwrap();
    let error = semantic::analyze_program(&expressions).unwrap_err();
    assert_eq!(error.kind, SemanticErrorKind::ReservedCanonName);
}

#[test]
fn ukrainian_canon_name_cannot_be_let_bound() {
    let source = "(let ((перше 1)) перше)";
    let expressions = parser::parse(source).unwrap();
    let error = semantic::analyze_program(&expressions).unwrap_err();
    assert_eq!(error.kind, SemanticErrorKind::ReservedCanonName);
}

#[test]
fn builtin_as_value_still_works() {
    // (let ((f car)) (f '(1 2))) — name `f` is ordinary; value is Canon builtin
    let ast = Expr::List(vec![
        Expr::Symbol("let".to_string()),
        Expr::List(vec![Expr::List(vec![
            Expr::Symbol("f".to_string()),
            Expr::Symbol("car".to_string()),
        ])]),
        Expr::List(vec![
            Expr::Symbol("f".to_string()),
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::List(vec![Expr::Integer(1), Expr::Integer(2)]),
            ]),
        ]),
    ]);

    let ir = lower_expr(&ast).unwrap();
    match ir {
        Ir::Let { bindings, body: _ } => {
            assert_eq!(bindings[0].1, Ir::Builtin("CAR".to_string()));
        }
        _ => panic!("Expected Let"),
    }
}

#[test]
fn special_form_as_value_still_rejected() {
    // (let ((f quote)) (f 1)) — quote is syntax-only
    let ast = Expr::List(vec![
        Expr::Symbol("let".to_string()),
        Expr::List(vec![Expr::List(vec![
            Expr::Symbol("f".to_string()),
            Expr::Symbol("quote".to_string()),
        ])]),
        Expr::List(vec![Expr::Symbol("f".to_string()), Expr::Integer(1)]),
    ]);

    let err = lower_expr(&ast).unwrap_err();
    assert!(err.to_string().contains("special forms are not callable"));
}

#[test]
fn non_canon_builtin_remains_shadowable() {
    // + is not Canon 0+7
    let source = "(let ((+ 99)) +)";
    let expressions = parser::parse(source).unwrap();
    semantic::analyze_program(&expressions).unwrap();
    let ir = lower_expr(&expressions[0]).unwrap();
    match ir {
        Ir::Let { bindings, .. } => {
            assert_eq!(bindings[0].0, "+");
            assert_eq!(bindings[0].1, Ir::Int(99));
        }
        other => panic!("Expected Let, got {other:?}"),
    }
}
