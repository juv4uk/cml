use cml::ir::Ir;
use cml::{lower, parser};

fn assert_exact_q_ge_surface(source: &str) {
    let expressions = parser::parse(source).expect("exact-Q >= witness must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("admitted exact-Q >= witness must reach structural lowering");
    let [node] = lowered.as_slice() else {
        panic!("expected one lowered exact-Q comparison for {source}, got {lowered:?}");
    };

    match node {
        Ir::Prim { op, args } => {
            assert_eq!(format!("{op:?}"), "ExactQGe", "source: {source}");
            assert_eq!(args, &[Ir::Int(194), Ir::Int(128)], "source: {source}");
        }
        Ir::App { .. } => panic!(
            "semantic 1018 surface {source} is admitted but still lowers as generic App; real #130 decoder evidence requires one distinct ExactQGe IR"
        ),
        other => panic!(
            "semantic 1018 surface {source} must lower as exact-Q comparison IR, got {other:?}"
        ),
    }
}

#[test]
fn exact_q_ge_surfaces_reach_one_distinct_numeric_comparison_ir() {
    for source in ["(>= 194 128)", "(не-менше? 194 128)"] {
        assert_exact_q_ge_surface(source);
    }
}
