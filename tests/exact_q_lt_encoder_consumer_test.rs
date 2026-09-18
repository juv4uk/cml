use cml::build::{Observation, compile_and_run};
use cml::ir::Ir;
use cml::{lower, parser};

fn assert_exact_q_lt_surface(source: &str) {
    let expressions = parser::parse(source).expect("exact-Q < witness must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("admitted exact-Q < witness must reach structural lowering");
    let [node] = lowered.as_slice() else {
        panic!("expected one lowered exact-Q comparison for {source}, got {lowered:?}");
    };

    match node {
        Ir::Prim { op, args } => {
            assert_eq!(format!("{op:?}"), "ExactQLt", "source: {source}");
            assert_eq!(args, &[Ir::Int(42), Ir::Int(512)], "source: {source}");
        }
        Ir::App { .. } => panic!(
            "semantic 1014 surface {source} is admitted but still lowers as generic App; my-lisp#507 compiled encoder now reaches this exact blocker through Lisp-owned largest-chunk"
        ),
        other => panic!(
            "semantic 1014 surface {source} must lower as exact-Q comparison IR, got {other:?}"
        ),
    }
}

#[test]
fn exact_q_lt_surfaces_reach_one_distinct_numeric_comparison_ir() {
    for source in ["(< 42 512)", "(менше? 42 512)"] {
        assert_exact_q_lt_surface(source);
    }
}

#[test]
fn exact_q_lt_executes_as_numeric_one_for_encoder_integer_domain() {
    let observed = compile_and_run("(< 42 512)")
        .expect("C backend exact-Q < witness should build and run");
    assert_eq!(observed, Observation::Value("1".to_string()));
}
