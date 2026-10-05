use cml::ir::{Ir, Quoted};
use cml::{lower, parser};

#[test]
fn migration_three_part_cond_clause_reaches_ir_without_truthiness_rewrite() {
    // Migration CondMatch contract: (query expected-result body).
    // The middle form is result data to match explicitly, not code to execute
    // and not a generic truthy/falsy sentinel.
    let source = r#"
        (cond
          ((тотожне? (quote a) (quote a)) (1)
           (quote matched))
          ((тотожне? (quote a) (quote b)) (1)
           (quote impossible)))
    "#;

    let expressions = parser::parse(source).expect("migration three-part cond source must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("CML must preserve the migration three-part CondMatch shape without truthiness rewrite");

    let [Ir::CondMatch { branches }] = lowered.as_slice() else {
        panic!("migration three-part cond must have an explicit-match IR shape");
    };
    assert_eq!(branches.len(), 2);

    let (query, expected, body) = &branches[0];
    assert!(
        matches!(query, Ir::App { func, .. } if matches!(func.as_ref(), Ir::Sid(sid) if *sid == sens::sid!(00000011)))
    );
    assert!(matches!(body, Ir::Quote(Quoted::Sym { original, .. }) if original == "matched"));
    assert_eq!(expected, &Quoted::List(vec![Quoted::Int(1)]));
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
