use cml::ir::Ir;
use cml::{canon, lower, parser};

#[test]
fn exact_q_quotient_is_admitted_by_upstream_semantic_identity_10100() {
    // This is an admission/provenance witness only. Arithmetic meaning stays
    // upstream; native lowering is a separate backend concern (#587).
    assert_eq!(
        canon::callable_semantic_id("quotient"),
        Some(sens::sid!(00010100)),
        "stable upstream quotient surface must resolve through exact SID8 00010100"
    );
    assert_eq!(
        canon::callable_semantic_id("частка"),
        Some(sens::sid!(00010100)),
        "Ukrainian peer surface must resolve to the same semantic identity"
    );

    let operation = canon::find_operation_by_id(sens::sid!(00010100))
        .expect("admitted quotient identity must have compiler operation metadata");
    assert_eq!(operation.canonical_name, "quotient");
    assert_eq!(operation.cml_ir_projection, "Ir::App(Sid(00010100))");
    assert_eq!(operation.status, "partial");

    assert_ne!(
        canon::callable_semantic_id("quotient"),
        Some(sens::sid!(00001111)),
        "quotient must stay distinct from exact division"
    );
    assert_ne!(
        canon::callable_semantic_id("quotient"),
        Some(sens::sid!(00010011)),
        "quotient must stay distinct from mod"
    );

    let expressions = parser::parse("(quotient 8 2)").expect("quotient source must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("stable quotient identity must reach compiler lowering");

    let [node] = lowered.as_slice() else {
        panic!("expected one lowered quotient expression, got {lowered:?}");
    };

    match node {
        Ir::App { func, args } => {
            assert!(matches!(func.as_ref(), Ir::Sid(sid) if *sid == sens::sid!(00010100)));
            assert_eq!(args, &[Ir::Int(8), Ir::Int(2)]);
        }
        other => panic!("quotient must lower as exact SID8 App, got {other:?}"),
    }
}
