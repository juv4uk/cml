use cml::ast::Expr;
use cml::upstream_sid_bridge::{address_of, rewrite_calls_to_sid};
use my_lisp::semantic_registry_export::semantic_id_for_admitted_surface;

fn sid_address(surface: &str) -> String {
    address_of(
        semantic_id_for_admitted_surface(surface)
            .unwrap_or_else(|| panic!("{surface} must be registry-admitted for this witness")),
    )
}

#[test]
fn callsite_rewrite_only_rewrites_a_real_admitted_callee() {
    let mut expr = Expr::List(vec![
        Expr::Symbol("lambda".into()),
        Expr::List(vec![Expr::Symbol("list".into())]),
        Expr::List(vec![
            Expr::Symbol("cons".into()),
            Expr::Symbol("list".into()),
            Expr::List(vec![
                Expr::Symbol("quote".into()),
                Expr::Symbol("list".into()),
            ]),
        ]),
    ]);

    rewrite_calls_to_sid(&mut expr);

    let Expr::List(top) = expr else {
        panic!("lambda witness must remain a list");
    };
    assert_eq!(
        top[0],
        Expr::Symbol("lambda".into()),
        "syntax position is not an application callee"
    );

    let Expr::List(params) = &top[1] else {
        panic!("lambda parameter list must remain a list");
    };
    assert_eq!(
        params,
        &[Expr::Symbol("list".into())],
        "a lexical binding named like an admitted surface must remain lexical data"
    );

    let Expr::List(body) = &top[2] else {
        panic!("lambda body must remain an application");
    };
    assert_eq!(
        body[0],
        Expr::Symbol(sid_address("cons")),
        "a real admitted application callee must be rewritten to its Lisp-owned SID"
    );
    assert_eq!(
        body[1],
        Expr::Symbol("list".into()),
        "an argument variable named like an admitted surface must not be rewritten"
    );

    let Expr::List(quoted) = &body[2] else {
        panic!("quoted witness must remain a list");
    };
    assert_eq!(
        quoted[0],
        Expr::Symbol("quote".into()),
        "quote syntax must not be rewritten by the call-site pass"
    );
    assert_eq!(
        quoted[1],
        Expr::Symbol("list".into()),
        "quoted registry spelling is data and must not be rewritten"
    );
}

#[test]
fn callsite_rewrite_does_not_own_definition_identity() {
    let mut expr = Expr::List(vec![
        Expr::Symbol("define".into()),
        Expr::Symbol("list".into()),
        Expr::List(vec![
            Expr::Symbol("lambda".into()),
            Expr::List(vec![Expr::Symbol("x".into())]),
            Expr::Symbol("x".into()),
        ]),
    ]);

    rewrite_calls_to_sid(&mut expr);

    let Expr::List(items) = expr else {
        panic!("definition witness must remain a list");
    };
    assert_eq!(
        items[0],
        Expr::Symbol("define".into()),
        "define syntax must remain syntax"
    );
    assert_eq!(
        items[1],
        Expr::Symbol("list".into()),
        "definition identity is owned by key_definition_by_sid, not call-site rewriting"
    );
}
