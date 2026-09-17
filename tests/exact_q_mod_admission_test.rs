use cml::ir::Ir;
use cml::{canon, lower, parser};

#[test]
fn exact_q_mod_is_admitted_by_upstream_semantic_identity_1007() {
    // Upstream authority is the vendored my-lisp semantic registry:
    //   1007 (en mod stable) (uk остача stable) ...
    // This test owns no expected arithmetic answer. It only requires CML to
    // recognize the already-ratified language identity as a distinct compiler
    // operation rather than a free variable or an alias of /, quotient, or %.
    assert_eq!(
        canon::callable_semantic_id("mod"),
        Some("1007"),
        "stable upstream `mod` must resolve through semantic identity 1007"
    );
    assert_eq!(
        canon::callable_semantic_id("остача"),
        Some("1007"),
        "Ukrainian peer surface must resolve to the same semantic identity"
    );

    let operation = canon::find_operation_by_id("1007")
        .expect("admitted semantic 1007 must have compiler operation metadata");
    assert_eq!(operation.canonical_name, "mod");
    assert_eq!(operation.cml_ir_projection, "Ir::App(Builtin(\"mod\"))");
    assert_eq!(operation.status, "partial");

    let expressions = parser::parse("(mod 7 3)").expect("mod source must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("stable Canon semantic identity 1007 must reach compiler lowering");

    let [node] = lowered.as_slice() else {
        panic!("expected one lowered mod expression, got {lowered:?}");
    };

    match node {
        Ir::App { func, args } => {
            assert_eq!(func.as_ref(), &Ir::Builtin("mod".to_string()));
            assert_eq!(args, &[Ir::Int(7), Ir::Int(3)]);
        }
        other => panic!(
            "semantic 1007 must first lower as its own Canon builtin before backend admission, got {other:?}"
        ),
    }
}
