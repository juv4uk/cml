use cml::ir::Ir;
use cml::{lower, parser};

#[test]
fn exact_q_le_reaches_a_distinct_numeric_comparison_ir() {
    let expressions = parser::parse("(<= 128 191)").expect("exact-Q <= witness must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("admitted exact-Q <= witness must reach structural lowering");
    let [node] = lowered.as_slice() else {
        panic!("expected one lowered exact-Q comparison, got {lowered:?}");
    };

    match node {
        Ir::Prim { op, args } => {
            // #92 requires a distinct exact-Q comparison mechanism. Numeric
            // 1017 must not reuse atom identity 0003 (`PrimOp::Eq`) and must
            // not stay a generic App. The exact variant spelling is a local
            // compiler detail; this first slice names it explicitly so the
            // RED cannot go green through metadata-only admission.
            assert_eq!(format!("{op:?}"), "ExactQLe");
            assert_eq!(args, &[Ir::Int(128), Ir::Int(191)]);
        }
        Ir::App { .. } => panic!(
            "semantic 1017 is admitted but still lowers as generic App; #92 requires a distinct exact-Q comparison IR"
        ),
        other => panic!("semantic 1017 must lower as exact-Q comparison IR, got {other:?}"),
    }
}
