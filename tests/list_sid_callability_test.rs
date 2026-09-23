use cml::canon::callable_semantic_id;
use cml::ir::{Ir, PrimOp};
use cml::{lower, parser};

#[test]
fn list_surface_is_owned_by_upstream_sid_in_callability_map() {
    let expected = my_lisp::sid!(00100111);
    assert_eq!(callable_semantic_id("list"), Some(expected));
    assert_eq!(callable_semantic_id("LIST"), Some(expected));
}

#[test]
fn direct_list_call_lowers_through_the_registry_identity() {
    let forms = parser::parse("(list (quote A) (quote B))").expect("LIST witness must parse");
    let lowered = lower::lower_program(&forms).expect("LIST witness must lower");

    let [Ir::Prim {
        op: PrimOp::List,
        args,
    }] = lowered.as_slice()
    else {
        panic!("registry-admitted LIST call must lower to PrimOp::List: {lowered:?}");
    };

    assert_eq!(args.len(), 2);
}

#[test]
fn lexical_list_callable_wins_over_global_registry_identity() {
    let forms = parser::parse("(lambda (list) (list))").expect("lexical LIST witness must parse");
    let lowered = lower::lower_program(&forms).expect("lexical LIST witness must lower");

    let [Ir::Lambda { body, .. }] = lowered.as_slice() else {
        panic!("expected one lambda: {lowered:?}");
    };
    let Ir::App { func, args } = body.as_ref() else {
        panic!("lexically-bound LIST must remain an application: {body:?}");
    };

    assert_eq!(func.as_ref(), &Ir::Var("LIST".to_string()));
    assert!(args.is_empty());
}
