use cml::lower::lower_program;
use cml::parser::parse;

#[test]
fn canonical_three_part_cond_lowers_on_current_master() {
    let exprs = parse("(cond ((quote same) (quote same) (quote selected)))")
        .expect("canonical cond witness must parse");
    lower_program(&exprs)
        .expect("current CML must lower canonical three-part cond before #89 list walkers");
}
