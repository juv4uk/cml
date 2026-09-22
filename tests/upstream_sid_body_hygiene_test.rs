use cml::ast::Expr;
use cml::parser;
use cml::upstream_sid_bridge::{address_of, rewrite_calls_in_body};
use my_lisp::semantic_registry_export::semantic_id_for_admitted_surface;

fn parse_one(source: &str) -> Expr {
    let mut forms = parser::parse(source).expect("witness source must parse");
    assert_eq!(forms.len(), 1);
    forms.remove(0)
}

fn list_sid() -> String {
    address_of(
        semantic_id_for_admitted_surface("list")
            .expect("my-lisp registry must admit list for this witness"),
    )
}

#[test]
fn global_list_call_is_rewritten_but_definition_name_is_not() {
    let mut form = parse_one("(def list-wrapper (lambda () (list (quote A))))");
    rewrite_calls_in_body(&mut form);

    let Expr::List(def) = form else {
        panic!("expected def form");
    };
    assert_eq!(def[1], Expr::Symbol("list-wrapper".into()));

    let Expr::List(lambda) = &def[2] else {
        panic!("expected lambda value");
    };
    let Expr::List(call) = &lambda[2] else {
        panic!("expected list call");
    };
    assert_eq!(
        call[0],
        Expr::Symbol(list_sid()),
        "a real global registry call should be addressed by its Lisp-owned SID"
    );
}

#[test]
fn quoted_registry_spelling_remains_inert_data() {
    let mut form = parse_one("(def quoted-list (lambda () (quote (list A))))");
    rewrite_calls_in_body(&mut form);

    let Expr::List(def) = form else {
        panic!("expected def form");
    };
    let Expr::List(lambda) = &def[2] else {
        panic!("expected lambda value");
    };
    let Expr::List(quote) = &lambda[2] else {
        panic!("expected quote form");
    };
    assert_eq!(quote[0], Expr::Symbol("quote".into()));

    let Expr::List(payload) = &quote[1] else {
        panic!("expected quoted list payload");
    };
    assert_eq!(
        payload[0],
        Expr::Symbol("list".into()),
        "quoted payload is data and must never be rewritten to a SID"
    );
}

#[test]
fn lexical_callable_named_like_registry_surface_wins_over_global_sid() {
    let mut form = parse_one("(def lexical-list (lambda (list) (list)))");
    rewrite_calls_in_body(&mut form);

    let Expr::List(def) = form else {
        panic!("expected def form");
    };
    let Expr::List(lambda) = &def[2] else {
        panic!("expected lambda value");
    };

    let Expr::List(params) = &lambda[1] else {
        panic!("expected lambda parameter list");
    };
    assert_eq!(params, &[Expr::Symbol("list".into())]);

    let Expr::List(call) = &lambda[2] else {
        panic!("expected lexical call");
    };
    assert_eq!(
        call[0],
        Expr::Symbol("list".into()),
        "lexical callable binding must win over the global registry surface"
    );
}

#[test]
fn registry_spelled_definition_identity_is_not_rewritten_by_call_pass() {
    let mut form = parse_one("(def list (lambda (x) x))");
    rewrite_calls_in_body(&mut form);

    let Expr::List(def) = form else {
        panic!("expected def form");
    };
    assert_eq!(
        def[1],
        Expr::Symbol("list".into()),
        "definition identity belongs to key_definition_by_sid, not call-site rewriting"
    );
}
