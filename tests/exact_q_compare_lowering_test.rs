use cml::ir::{Ir, PrimOp};
use cml::{canon, lower, parser};

#[test]
fn exact_q_compare_family_is_admitted_by_distinct_semantic_identity() {
    // Upstream authority: my-lisp contracts/exact-q-binary-contract.lisp.
    // The family remains distinct from semantic 0003 atom identity. #92's
    // first GREEN slice promotes only 1017 (<=) to explicit comparison IR;
    // the remaining identities stay admitted-but-partial generic Apps.
    let cases = [
        ("<", "1014"),
        (">", "1015"),
        ("=", "1016"),
        ("<=", "1017"),
        (">=", "1018"),
    ];

    for (surface, semantic_id) in cases {
        assert_eq!(
            canon::callable_semantic_id(surface),
            Some(semantic_id),
            "{surface} must resolve through its exact-Q semantic identity"
        );

        let operation = canon::find_operation_by_id(semantic_id)
            .expect("admitted exact-Q comparison identity must have compiler operation metadata");
        assert_eq!(operation.canonical_name, surface);
        assert_eq!(operation.status, "partial");

        let source = format!("({surface} 128 191)");
        let expressions = parser::parse(&source).expect("exact-Q comparison source must parse");
        let lowered = lower::lower_program(&expressions)
            .expect("a stable Canon comparison identity must reach compiler lowering");

        let [node] = lowered.as_slice() else {
            panic!("expected one lowered expression for {surface}, got {lowered:?}");
        };

        if semantic_id == "1017" {
            assert_eq!(operation.cml_ir_projection, "Ir::Prim(PrimOp::ExactQLe)");
            assert!(matches!(
                node,
                Ir::Prim {
                    op: PrimOp::ExactQLe,
                    args
                } if args == &[Ir::Int(128), Ir::Int(191)]
            ));
        } else {
            assert_eq!(
                operation.cml_ir_projection,
                format!("Ir::App(Builtin(\"{surface}\"))")
            );
            match node {
                Ir::App { func, args } => {
                    assert_eq!(
                        func.as_ref(),
                        &Ir::Builtin(surface.to_string()),
                        "numeric {surface} must be a Canon builtin identity, not a free Var or semantic 0003 Eq"
                    );
                    assert_eq!(args, &[Ir::Int(128), Ir::Int(191)]);
                }
                other => panic!(
                    "semantic identity {semantic_id} must remain admitted-but-partial until its own #92 slice, got {other:?}"
                ),
            }
        }
    }
}
