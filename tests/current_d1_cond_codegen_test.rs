use cml::c_backend::{CBackend, CConditionalMechanism};
use cml::ir::Ir;

fn one_branch_cond() -> Ir {
    Ir::Cond {
        branches: vec![(Ir::Int(1), Ir::Int(7))],
    }
}

fn main_section(source: &str) -> &str {
    source
        .split_once("int main(void) {")
        .map(|(_, main)| main)
        .expect("generated C must contain main")
}

#[test]
fn compatibility_mode_remains_the_default() {
    let mut backend = CBackend::new();
    assert_eq!(
        backend.conditional_mechanism(),
        CConditionalMechanism::CompatibilityTruthiness
    );

    let source = backend
        .compile_program(&[one_branch_cond()])
        .expect("compatibility conditional codegen");
    let main = main_section(&source);

    assert!(main.contains("if (truthy(mk_int(1)))"));
    assert!(!main.contains("require_predicate_bit(mk_int(1), \"current-cond\")"));
}

#[test]
fn current_exact_d1_mode_uses_only_the_exact_predicate_gate() {
    let mut backend = CBackend::new().with_current_d1_conditional();
    assert_eq!(
        backend.conditional_mechanism(),
        CConditionalMechanism::CurrentExactD1
    );

    let source = backend
        .compile_program(&[one_branch_cond()])
        .expect("current D1 conditional codegen");
    let main = main_section(&source);

    assert!(main.contains(
        "if (require_predicate_bit(mk_int(1), \"current-cond\"))"
    ));
    assert!(
        !main.contains("truthy("),
        "current exact D1 conditional path must not use legacy truthiness: {main}"
    );
    assert!(main.contains("else { _c = &NIL_V; }"));
}
