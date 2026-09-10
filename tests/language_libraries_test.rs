//! COMPILER-07 — language-owned library slice through the compile pipeline.
//!
//! A nontrivial real core.my-shaped fragment (length / map) is defined in
//! source, lowered, emitted to C, and executed — not reimplemented as C
//! runtime helpers. Mutual/self recursion relies on two-pass top-level defs
//! (COMPILER-04/12).

use cml::build::{compile_and_run, Observation};

fn value(source: &str) -> String {
    match compile_and_run(source).expect("compile_and_run") {
        Observation::Value(v) => v,
        other => panic!("expected Value, got {other:?}\nsource:\n{source}"),
    }
}

/// Classic length-onto / length pair (same shape as CML-LENGTH-E2E evidence).
const LENGTH_LIB: &str = r#"
(def length-onto
  (lambda (x acc)
    (cond ((atom x) acc)
          (t (length-onto (cdr x) (+ acc 1))))))
(def length
  (lambda (x) (length-onto x 0)))
"#;

const MAP_LIB: &str = r#"
(def map
  (lambda (f xs)
    (cond ((atom xs) ())
          (t (cons (f (car xs)) (map f (cdr xs)))))))
"#;

#[test]
fn length_of_quoted_list_is_three() {
    let src = format!(
        "{LENGTH_LIB}\n(length (quote (a b c)))"
    );
    assert_eq!(value(&src), "3");
}

#[test]
fn length_of_empty_list_is_zero() {
    let src = format!("{LENGTH_LIB}\n(length (quote ()))");
    assert_eq!(value(&src), "0");
}

#[test]
fn length_of_nested_structure_counts_top_level_only() {
    let src = format!(
        "{LENGTH_LIB}\n(length (quote ((a b) c)))"
    );
    assert_eq!(value(&src), "2");
}

#[test]
fn map_add1_over_quoted_list() {
    let src = format!(
        "{MAP_LIB}\n(map (lambda (x) (+ x 1)) (quote (1 2 3)))"
    );
    assert_eq!(value(&src), "(2 3 4)");
}

#[test]
fn map_over_empty_is_nil() {
    let src = format!(
        "{MAP_LIB}\n(map (lambda (x) (+ x 1)) (quote ()))"
    );
    assert_eq!(value(&src), "()");
}

#[test]
fn length_composed_with_map() {
    let src = format!(
        "{LENGTH_LIB}\n{MAP_LIB}\n(length (map (lambda (x) x) (quote (a b c d))))"
    );
    assert_eq!(value(&src), "4");
}
