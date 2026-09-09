//! Contract 5.0 decimal-separator reader tests.
//!
//! Dot and comma are equivalent decimal separators only for an otherwise
//! valid finite decimal / scientific numeral. Non-numeric tokens that
//! happen to contain a comma remain ordinary symbols.

use cml::ast::Expr;
use cml::parser::parse;

fn single(source: &str) -> Expr {
    let mut exprs = parse(source).unwrap_or_else(|e| panic!("parse `{source}`: {e:?}"));
    assert_eq!(exprs.len(), 1, "expected one expression from `{source}`");
    exprs.pop().unwrap()
}

#[test]
fn comma_and_dot_denote_the_same_rational() {
    assert_eq!(single("12.455"), single("12,455"));
    assert_eq!(single("-0.25"), single("-0,25"));
    assert_eq!(single("1.5"), single("1,5"));
}

#[test]
fn scientific_notation_accepts_comma_mantissa() {
    // 1.5e3 ≡ 1,5e3 ≡ 1500
    assert_eq!(single("1.5e3"), Expr::Integer(1500));
    assert_eq!(single("1,5e3"), Expr::Integer(1500));
    assert_eq!(single("1.5E3"), Expr::Integer(1500));
}

#[test]
fn fractional_decimal_becomes_rational() {
    assert_eq!(single("0.5"), Expr::Rational(1, 2));
    assert_eq!(single("0,5"), Expr::Rational(1, 2));
    // gcd-reduced: 12455/1000 = 2491/200
    assert_eq!(single("12.455"), Expr::Rational(2491, 200));
    assert_eq!(single("12,455"), Expr::Rational(2491, 200));
}

#[test]
fn negative_and_signed_exponents() {
    assert_eq!(single("-0,25"), Expr::Rational(-1, 4));
    assert_eq!(single("1,5e-1"), Expr::Rational(3, 20)); // 0.15 = 3/20
}

#[test]
fn non_numeric_tokens_with_comma_remain_symbols() {
    assert_eq!(single("а,б"), Expr::Symbol("а,б".to_string()));
    assert_eq!(single("версія1,2"), Expr::Symbol("версія1,2".to_string()));
    assert_eq!(single("1,2,3"), Expr::Symbol("1,2,3".to_string()));
}

#[test]
fn malformed_decimals_remain_symbols() {
    // Trailing separator with empty fraction is rejected.
    assert_eq!(single("1."), Expr::Symbol("1.".to_string()));
    assert_eq!(single("1,"), Expr::Symbol("1,".to_string()));
    // Leading separator is a valid mantissa (".5" ≡ 1/2), matching common
    // exact-decimal readers including my-lisp's path for well-formed tokens.
    assert_eq!(single(".5"), Expr::Rational(1, 2));
    assert_eq!(single(",5"), Expr::Rational(1, 2));
    assert_eq!(single("1ee3"), Expr::Symbol("1ee3".to_string()));
    assert_eq!(single("1e"), Expr::Symbol("1e".to_string()));
}

#[test]
fn integers_and_slash_rationals_unchanged() {
    assert_eq!(single("42"), Expr::Integer(42));
    assert_eq!(single("-7"), Expr::Integer(-7));
    assert_eq!(single("1/2"), Expr::Rational(1, 2));
    assert_eq!(single("-3/4"), Expr::Rational(-3, 4));
}

#[test]
fn existing_apostrophe_contract_still_holds() {
    assert_eq!(
        single("'кіт"),
        Expr::List(vec![
            Expr::Symbol("quote".to_string()),
            Expr::Symbol("кіт".to_string()),
        ])
    );
    assert_eq!(single("об'єкт"), Expr::Symbol("об'єкт".to_string()));
}
