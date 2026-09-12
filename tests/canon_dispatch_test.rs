//! cml#9 Finding 1: `lower.rs` used to recognize quote/cond/lambda/define as
//! special forms only by their hardcoded English spelling. A source written
//! with the Ukrainian Canon surface (as-є / за-умовою / функція / визначити,
//! from the real my-lisp semantic-registry.wsm) fell through to a generic
//! call against an unbound symbol instead of being lowered as the special
//! form it actually is. These tests prove the fix: real parse -> lower
//! through the ordinary pipeline, in Ukrainian, producing the same IR shape
//! the English spelling produces.

use cml::ir::Ir;
use cml::lower::lower_program;
use cml::parser;

#[test]
fn ukrainian_quote_lowers_to_ir_quote() {
    let expressions = parser::parse("(як-є (1 2))").unwrap();
    let program = lower_program(&expressions).unwrap();
    assert!(matches!(program[0], Ir::Quote(_)));
}

#[test]
fn ukrainian_cond_lowers_to_ir_cond() {
    let expressions = parser::parse("(за-умовою (t 1))").unwrap();
    let program = lower_program(&expressions).unwrap();
    assert!(matches!(program[0], Ir::Cond { .. }));
}

#[test]
fn ukrainian_lambda_lowers_to_ir_lambda() {
    let expressions = parser::parse("(функція (x) x)").unwrap();
    let program = lower_program(&expressions).unwrap();
    assert!(matches!(program[0], Ir::Lambda { .. }));
}

#[test]
fn ukrainian_define_lowers_to_ir_def() {
    let expressions = parser::parse("(визначити f (функція (x) x))").unwrap();
    let program = lower_program(&expressions).unwrap();
    match &program[0] {
        Ir::Def { name, .. } => assert_eq!(name, "F"),
        other => panic!("expected Ir::Def, got {other:?}"),
    }
}

#[test]
fn ukrainian_lambda_as_a_bare_callable_value_is_rejected_same_as_english() {
    // Same rule `special_form_as_value_still_rejected` proves for English
    // `quote`: a Canon special-form spelling used as a plain value (not in
    // call position) must be rejected, in every Canon-registered language.
    let expressions = parser::parse("(за-умовою ((eq 1 1) функція))").unwrap();
    let err = lower_program(&expressions).unwrap_err();
    assert!(err.to_string().contains("special forms are not callable"));
}

#[test]
fn english_and_ukrainian_quote_produce_the_same_ir() {
    let english = lower_program(&parser::parse("(quote (1 2))").unwrap()).unwrap();
    let ukrainian = lower_program(&parser::parse("(як-є (1 2))").unwrap()).unwrap();
    assert_eq!(english, ukrainian);
}
