use cml::ir::Ir;
use cml::{canon, lower, parser};

#[test]
fn exact_q_compare_family_is_admitted_by_distinct_semantic_identity() {
    // Upstream authority: my-lisp contracts/exact-q-binary-contract.lisp.
    // The family remains distinct from semantic 0003 atom identity. #92's
    // Three GREEN slices promote 1014 (<), 1017 (<=), and 1018 (>=) to
    // explicit comparison IR; 1015/1016 remain admitted-but-partial generic Apps.
    let cases = [
        ("<", sens::sid!(00011010)),
        (">", sens::sid!(00011011)),
        ("=", sens::sid!(00011100)),
        ("<=", sens::sid!(00011101)),
        (">=", sens::sid!(00011110)),
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

        // #246: every admitted callable is an exact SID8 function call.
        let expected_projection = format!("Ir::App(Sid({}))", semantic_id);
        assert_eq!(operation.cml_ir_projection, expected_projection);
        match node {
            Ir::App { func, args } => {
                assert!(
                    matches!(func.as_ref(), Ir::Sid(sid) if *sid == semantic_id),
                    "{surface} must lower as an exact SID8 call, not a Builtin or free Var"
                );
                assert_eq!(args, &[Ir::Int(128), Ir::Int(191)]);
            }
            other => {
                panic!("semantic identity {semantic_id} must lower as Ir::App(Sid), got {other:?}")
            }
        }
    }
}
