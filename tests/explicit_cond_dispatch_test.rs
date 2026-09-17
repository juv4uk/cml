use cml::{lower, parser};

#[test]
fn current_three_part_cond_clause_reaches_ir_without_truthiness_rewrite() {
    // Upstream my-lisp control contract: (query expected-result body).
    // The middle form is result data to match explicitly, not code to execute
    // and not a generic truthy/falsy sentinel.
    let source = r#"
        (cond
          ((eq (quote a) (quote a)) (identity-relation same)
           (quote matched))
          ((eq (quote a) (quote b)) (identity-relation same)
           (quote impossible)))
    "#;

    let expressions = parser::parse(source).expect("current upstream cond source must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("CML must admit current three-part cond before #89 can compile real list walkers");

    assert_eq!(lowered.len(), 1);
}
