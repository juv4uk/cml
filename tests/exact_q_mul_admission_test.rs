use cml::ir::Ir;
use cml::{canon, lower, parser};

#[test]
fn exact_q_mul_is_admitted_by_upstream_semantic_identity_1002() {
    for surface in ["*", "помножити", "guṇana"] {
        assert_eq!(
            canon::callable_semantic_id(surface),
            Some(my_lisp::sid!(00001110)),
            "stable upstream multiplication surface {surface:?} must resolve through semantic identity 1002"
        );
    }

    let operation = canon::find_operation_by_id(my_lisp::sid!(00001110))
        .expect("admitted exact-Q multiplication identity must have compiler operation metadata");
    assert_eq!(operation.semantic_id, my_lisp::sid!(00001110));
    assert_eq!(operation.canonical_name, "*");
    assert_eq!(operation.cml_ir_projection, "Ir::App(Sid(00001110))");
    assert_eq!(operation.status, "partial");

    let expressions = parser::parse("(* 3 64)").expect("multiplication source must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("stable multiplication identity must reach compiler lowering");

    let [node] = lowered.as_slice() else {
        panic!("expected one lowered multiplication expression, got {lowered:?}");
    };

    match node {
        Ir::App { func, args } => {
            assert!(
                matches!(func.as_ref(), Ir::Sid(sid) if *sid == my_lisp::sid!(00001110)),
                "semantic 1002 must lower as its own SID8 identity"
            );
            assert_eq!(args, &[Ir::Int(3), Ir::Int(64)]);
        }
        other => panic!("semantic identity 1002 must lower to a distinct SID8 App, got {other:?}"),
    }
}
