use cml::ast::Expr;
use cml::ir::Ir;
use cml::lower::lower_expr;

#[test]
fn test_builtin_shadowing() {
    // (let ((car 1)) (car '(1 2)))
    let ast = Expr::List(vec![
        Expr::Symbol("let".to_string()),
        Expr::List(vec![Expr::List(vec![
            Expr::Symbol("car".to_string()),
            Expr::Integer(1),
        ])]),
        Expr::List(vec![
            Expr::Symbol("car".to_string()),
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::List(vec![Expr::Integer(1), Expr::Integer(2)]),
            ]),
        ]),
    ]);

    let ir = lower_expr(&ast).unwrap();
    match ir {
        Ir::Let { bindings, body } => {
            assert_eq!(bindings[0].0, "CAR");
            match *body {
                Ir::App { func, args: _ } => {
                    assert_eq!(*func, Ir::Var("CAR".to_string()));
                }
                _ => panic!("Expected App, got {:?}", body),
            }
        }
        _ => panic!("Expected Let, got {:?}", ir),
    }
}

#[test]
fn test_builtin_as_value() {
    // (let ((f car)) (f '(1 2)))
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
fn test_special_form_as_value() {
    // (let ((f quote)) (f 1))
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
