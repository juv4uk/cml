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
fn lambda_parameter_named_t_shadows_the_literal_true() {
    // Raised by my-lisp while reviewing a cml x86 witness: `t` is not in
    // the Canon 0+7 reserved set (unlike `car`/`cons`/etc), so my-lisp
    // treats it as an ordinary shadowable lexical binding, not a reserved
    // literal. Before this fix, lower_symbol hardcoded `T` -> Ir::True
    // unconditionally, so `(lambda (t) t)` silently ignored its own
    // parameter and always returned the literal true -- a real semantic
    // divergence from my-lisp with no error at all.
    let source = "(def f (lambda (t) t)) (f (quote hi))";
    let expressions = parser::parse(source).unwrap();
    let program = lower_expr(&expressions[0]).unwrap();
    match program {
        Ir::Def { value, .. } => match *value {
            Ir::Lambda { body, .. } => {
                assert_eq!(*body, Ir::Var("T".to_string()));
            }
            other => panic!("expected Lambda, got {other:?}"),
        },
        other => panic!("expected Def, got {other:?}"),
    }
}

#[test]
fn lambda_parameter_named_nil_shadows_the_literal_nil() {
    let source = "(lambda (nil) nil)";
    let expressions = parser::parse(source).unwrap();
    let ir = lower_expr(&expressions[0]).unwrap();
    match ir {
        Ir::Lambda { body, .. } => assert_eq!(*body, Ir::Var("NIL".to_string())),
        other => panic!("expected Lambda, got {other:?}"),
    }
}

#[test]
fn unbound_t_and_nil_still_lower_to_the_literal() {
    // The fix must not break the ordinary, unshadowed case.
    let source = "(quote ()) t nil";
    let expressions = parser::parse(source).unwrap();
    assert_eq!(lower_expr(&expressions[1]).unwrap(), Ir::True);
    assert_eq!(lower_expr(&expressions[2]).unwrap(), Ir::Nil);
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
