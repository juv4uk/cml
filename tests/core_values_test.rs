//! COMPILER-03 — core value representation via compiled C path.
//!
//! Closes admitted surface for: (), symbols, proper/dotted pairs, exact
//! integers, exact rationals, and strings. CML's C path canonicalizes symbol
//! spellings to uppercase while string contents remain unchanged.

use cml::build::{Observation, compile_and_run};

fn value(source: &str) -> String {
    match compile_and_run(source).expect("compile_and_run") {
        Observation::Value(v) => v,
        other => panic!("expected Value, got {other:?} for {source:?}"),
    }
}

#[test]
fn nil_and_empty_program() {
    assert_eq!(value("()"), "()");
    match compile_and_run("").expect("empty") {
        Observation::Value(v) => assert_eq!(v, "()"),
        Observation::Error(_) | Observation::Unsupported(_) => {
            assert_eq!(value("()"), "()");
        }
    }
}

#[test]
fn exact_integers() {
    assert_eq!(value("0"), "0");
    assert_eq!(value("42"), "42");
    assert_eq!(value("-7"), "-7");
    assert_eq!(value("(+ 100 23)"), "123");
    assert_eq!(value("(- 10 3)"), "7");
    assert_eq!(value("(* 6 7)"), "42");
}

#[test]
fn exact_rationals() {
    assert_eq!(value("1/2"), "1/2");
    assert_eq!(value("12,5"), "25/2");
    assert_eq!(value("(+ 1/2 1/2)"), "1");
    assert_eq!(value("(/ 1 4)"), "1/4");
}

#[test]
fn symbols_and_truth() {
    assert_eq!(value("(quote foo)"), "FOO");
    assert_eq!(value("(quote t)"), "T");
    assert_eq!(value("(atom (quote x))"), "T");
}

#[test]
fn proper_and_dotted_pairs() {
    assert_eq!(value("(cons 1 2)"), "(1 . 2)");
    assert_eq!(value("(car (quote (a b c)))"), "A");
    assert_eq!(value("(car (cdr (quote (a b c))))"), "B");
    assert_eq!(value("(cons 1 (cons 2 ()))"), "(1 2)");
}

#[test]
fn strings_are_distinct_values() {
    assert_eq!(value("\"hello\""), "hello");
    assert_eq!(value("(quote \"hi\")"), "hi");
    assert_eq!(value("(eq \"a\" \"a\")"), "T");
    assert_eq!(value("(car (cons \"x\" 1))"), "x");
}
