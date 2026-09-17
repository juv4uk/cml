use cml::ir::Ir;
use cml::{canon, lower, parser};

#[test]
fn exact_q_mul_is_admitted_by_upstream_semantic_identity_1002() {
    for surface in ["*", "помножити", "guṇana"] {
        assert_eq!(
            canon::callable_semantic_id(surface),
            Some("1002"),
            "stable upstream multiplication surface {surface:?} must resolve through semantic identity 1002"
        );
    }

    let operation = canon::find_operation_by_id("1002")
        .expect("admitted exact-Q multiplication identity must have compiler operation metadata");
    assert_eq!(operation.semantic_id, "1002");
    assert_eq!(operation.canonical_name, "*");
    assert_eq!(operation.cml_ir_projection, "Ir::App(Builtin(\"*\"))");
    assert_eq!(operation.status, "partial");

    let expressions = parser::parse("(* 3 64)").expect("multiplication source must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("stable multiplication identity must reach compiler lowering");

    let [node] = lowered.as_slice() else {
        panic!("expected one lowered multiplication expression, got {lowered:?}");
    };

    match node {
        Ir::App { func, args } => {
            assert_eq!(
                func.as_ref(),
                &Ir::Builtin("*".to_string()),
                "semantic 1002 must lower as its own Canon builtin identity"
            );
            assert_eq!(args, &[Ir::Int(3), Ir::Int(64)]);
        }
        other => panic!(
            "semantic identity 1002 must lower to a distinct Canon builtin before backend admission, got {other:?}"
        ),
    }
}
