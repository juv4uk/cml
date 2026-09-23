use cml::ast::Expr;
use cml::parser::parse;

#[test]
fn expression_initial_apostrophe_desugars_to_quote() {
    assert_eq!(
        parse("'кіт").expect("reader should accept quote sugar"),
        vec![Expr::List(vec![
            Expr::Symbol("quote".to_string()),
            Expr::Symbol("кіт".to_string()),
        ])]
    );
}

#[test]
fn internal_apostrophe_remains_part_of_identifier() {
    assert_eq!(
        parse("об'єкт п'ять зв'язок").expect("Ukrainian identifiers should parse"),
        vec![
            Expr::Symbol("об'єкт".to_string()),
            Expr::Symbol("п'ять".to_string()),
            Expr::Symbol("зв'язок".to_string()),
        ]
    );
}

#[test]
fn quoted_identifier_may_itself_contain_an_apostrophe() {
    assert_eq!(
        parse("'об'єкт").expect("quote sugar should coexist with internal apostrophe"),
        vec![Expr::List(vec![
            Expr::Symbol("quote".to_string()),
            Expr::Symbol("об'єкт".to_string()),
        ])]
    );
}

#[test]
fn nested_quote_sugar_is_structural_not_a_new_primitive() {
    assert_eq!(
        parse("''кіт").expect("nested reader sugar should parse"),
        vec![Expr::List(vec![
            Expr::Symbol("quote".to_string()),
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::Symbol("кіт".to_string()),
            ]),
        ])]
    );
}


#[test]
fn exact_bare_sid8_is_reserved_before_generic_integer_parsing() {
    assert_eq!(
        parse("00000101").expect("exact bare SID8 must parse"),
        vec![Expr::Sid(my_lisp::sid!(00000101))],
    );
}

#[test]
fn quoted_eight_bit_text_remains_string_data() {
    assert_eq!(
        parse("\"00000101\"").expect("quoted bit text must parse as String"),
        vec![Expr::String("00000101".to_string())],
    );
}

#[test]
fn decimal_integer_does_not_alias_sid8() {
    assert_eq!(
        parse("101").expect("decimal integer must parse"),
        vec![Expr::Integer(101)],
    );
}
