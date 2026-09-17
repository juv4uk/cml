use cml::lower::lower_program;
use cml::parser::parse;

#[test]
fn canonical_three_part_cond_lowers_on_current_master() {
    // Current my-lisp Canon control is (query expected-result body):
    // expected-result is inert data, not a truthiness sentinel.
    let exprs = parse("(cond ((quote same) (quote same) (quote selected)))")
        .expect("canonical cond witness must parse");

    let lowered = lower_program(&exprs)
        .expect("current CML must lower canonical three-part cond before #89 list walkers");

    let debug = format!("{lowered:#?}");
    assert!(debug.contains("selected") || debug.contains("SELECTED"));
}
