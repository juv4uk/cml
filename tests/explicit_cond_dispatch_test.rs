use cml::ir::{Ir, PrimOp, Quoted};
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

    let [Ir::CondMatch { branches }] = lowered.as_slice() else {
        panic!("current three-part cond must have an explicit-match IR shape");
    };
    assert_eq!(branches.len(), 2);

    let (query, expected, body) = &branches[0];
    assert!(matches!(query, Ir::Prim { op: PrimOp::Eq, .. }));
    assert!(matches!(body, Ir::Quote(Quoted::Sym { original, .. }) if original == "matched"));
    assert_eq!(
        expected,
        &Quoted::List(vec![
            Quoted::Sym {
                uppercased: "IDENTITY-RELATION".to_string(),
                original: "identity-relation".to_string(),
            },
            Quoted::Sym {
                uppercased: "SAME".to_string(),
                original: "same".to_string(),
            },
        ])
    );
}

#[test]
fn canonical_and_migration_cond_clauses_cannot_be_mixed() {
    let source = r#"
        (cond
          ((quote x) x (quote canonical))
          (t (quote historical)))
    "#;
    let expressions = parser::parse(source).unwrap();
    let error =
        lower::lower_program(&expressions).expect_err("mixed control models must fail closed");
    assert!(
        error
            .detail
            .contains("cannot mix canonical three-part clauses"),
        "unexpected lowering error: {error}"
    );
}
