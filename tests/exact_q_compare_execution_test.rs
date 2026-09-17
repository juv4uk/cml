use cml::ir::Ir;
use cml::{lower, parser};

fn assert_exact_q_le_surface(source: &str) {
    let expressions = parser::parse(source).expect("exact-Q <= witness must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("admitted exact-Q <= witness must reach structural lowering");
    let [node] = lowered.as_slice() else {
        panic!("expected one lowered exact-Q comparison for {source}, got {lowered:?}");
    };

    match node {
        Ir::Prim { op, args } => {
            // #92 requires one distinct exact-Q comparison mechanism for the
            // opaque semantic identity 1017. Neither the symbolic nor the UK
            // surface may own a separate lowering rule, reuse atom identity
            // 0003 (`PrimOp::Eq`), or remain a generic App.
            assert_eq!(format!("{op:?}"), "ExactQLe", "source: {source}");
            assert_eq!(args, &[Ir::Int(128), Ir::Int(191)], "source: {source}");
        }
        Ir::App { .. } => panic!(
            "semantic 1017 surface {source} is admitted but still lowers as generic App; #92 requires one distinct exact-Q comparison IR"
        ),
        other => panic!(
            "semantic 1017 surface {source} must lower as exact-Q comparison IR, got {other:?}"
        ),
    }
}

#[test]
fn exact_q_le_surfaces_reach_one_distinct_numeric_comparison_ir() {
    for source in ["(<= 128 191)", "(не-більше? 128 191)"] {
        assert_exact_q_le_surface(source);
    }
}
