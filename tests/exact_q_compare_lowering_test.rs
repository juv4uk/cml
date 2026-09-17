use cml::ir::Ir;
use cml::{canon, lower, parser};

#[test]
fn exact_q_less_equal_is_admitted_by_semantic_identity_without_atom_eq_conflation() {
    // Upstream authority: my-lisp contracts/exact-q-binary-contract.lisp,
    // semantic identity 1017. This first vertical slice proves identity
    // admission only; backend execution remains deliberately unsupported.
    assert_eq!(canon::callable_semantic_id("<="), Some("1017"));

    let operation = canon::find_operation_by_id("1017")
        .expect("admitted exact-Q <= identity must have compiler operation metadata");
    assert_eq!(operation.canonical_name, "<=");
    assert_eq!(operation.cml_ir_projection, "Ir::App(Builtin(\"<=\"))");
    assert_eq!(operation.status, "partial");

    let expressions = parser::parse("(<= 128 191)").expect("exact-Q comparison source must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("a stable Canon comparison identity must reach compiler lowering");

    let [node] = lowered.as_slice() else {
        panic!("expected one lowered expression, got {lowered:?}");
    };

    match node {
        Ir::App { func, args } => {
            assert_eq!(
                func.as_ref(),
                &Ir::Builtin("<=".to_string()),
                "numeric <= must be a Canon builtin identity, not a free Var or semantic 0003 Eq"
            );
            assert_eq!(args, &[Ir::Int(128), Ir::Int(191)]);
        }
        other => panic!(
            "semantic identity 1017 must lower to a distinct Canon builtin before backend admission, got {other:?}"
        ),
    }
}
