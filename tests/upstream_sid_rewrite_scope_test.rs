use cml::ast::Expr;
use cml::upstream_sid_bridge::rewrite_calls_to_sid;
use my_lisp::semantic_registry_export::{
    semantic_id_bits, semantic_id_for_admitted_surface,
};

fn sid_address(surface: &str) -> String {
    let sid = semantic_id_for_admitted_surface(surface)
        .unwrap_or_else(|| panic!("upstream registry must admit {surface}"));
    semantic_id_bits(sid)
}

#[test]
fn rewrite_changes_real_call_head_but_not_quoted_payload() {
    let mut form = Expr::List(vec![
        Expr::Symbol("list".into()),
        Expr::List(vec![
            Expr::Symbol("quote".into()),
            Expr::Symbol("list".into()),
        ]),
    ]);

    rewrite_calls_to_sid(&mut form);

    assert_eq!(
        form,
        Expr::List(vec![
            Expr::Symbol(sid_address("list")),
            Expr::List(vec![
                Expr::Symbol("quote".into()),
                Expr::Symbol("list".into()),
            ]),
        ]),
        "SID rewrite may address a real callable head, but quote is syntax and its payload is inert data"
    );
}

#[test]
fn rewrite_respects_lexical_shadowing_of_registry_spelling() {
    let mut form = Expr::List(vec![
        Expr::Symbol("lambda".into()),
        Expr::List(vec![Expr::Symbol("list".into())]),
        Expr::List(vec![Expr::Symbol("list".into())]),
    ]);

    let expected = form.clone();
    rewrite_calls_to_sid(&mut form);

    assert_eq!(
        form, expected,
        "a lambda parameter named like a registry callable is lexical identity; neither binder nor shadowed call may be rewritten to the global SID"
    );
}
