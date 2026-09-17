use cml::compiler::Compiler;
use cml::{lower, parser};

#[test]
fn current_three_part_cond_is_admitted_and_no_match_materializes_canon_zero() {
    // Upstream control authority: each canonical clause is
    // (query expected-result body). The expected result is data, not a
    // truthiness sentinel. With no matching clause the result is Canon 0 ().
    let source = r#"
        (cond
          ((quote (identity-relation distinct)) (identity-relation same)
           (quote unreachable)))
    "#;

    let expressions = parser::parse(source).expect("canonical cond source must parse");
    let program = lower::lower_program(&expressions)
        .expect("current three-part cond must lower before real Lisp walkers can compile");
    let assembly = Compiler::new()
        .compile(&program)
        .expect("FPGA backend must implement canonical explicit-result matching");

    assert!(
        assembly.contains("EQ R15 R12 R13\ncond_match_end_"),
        "canonical no-match must materialize Canon 0 in the result register immediately before the end label; assembly was:\n{assembly}"
    );
}

#[test]
fn canonical_and_migration_cond_models_cannot_be_mixed() {
    let source = r#"
        (cond
          ((quote x) x (quote canonical))
          (t (quote historical)))
    "#;

    let expressions = parser::parse(source).expect("mixed cond source must parse");
    let error = lower::lower_program(&expressions)
        .expect_err("canonical three-part and migration two-part clauses must fail closed when mixed");

    assert!(
        error.detail.contains("cannot mix canonical three-part clauses"),
        "unexpected mixed-control error: {error}"
    );
}
