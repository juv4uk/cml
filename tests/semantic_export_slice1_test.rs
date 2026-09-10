//! End-to-end: my-lisp semantic export slice-1 program shape through CML C path.
//!
//! Fixture #69 shape (named def + recursion):
//!   (def count-down (lambda (n) (cond ((eq n 0) (quote done)) (t (count-down (- n 1))))))
//!   (count-down N)
//!
//! Forms covered by export: quote, cond, lambda, define, eq, subtraction.

use cml::build::{compile_and_run, Observation};
use cml::semantic_export::{parse_export, validate_slice1};

const EXPORT: &str = include_str!("../contracts/mylisp-cml-export.wsm");

#[test]
fn export_slice1_validates() {
    let export = parse_export(EXPORT).expect("parse export");
    validate_slice1(&export).expect("validate slice1");
}

#[test]
fn count_down_compiles_and_returns_done() {
    // Small N so the test stays fast; same shape as fixture #69.
    let src = r#"
(def count-down
  (lambda (n)
    (cond ((eq n 0) (quote done))
          (t (count-down (- n 1))))))
(count-down 5)
"#;
    match compile_and_run(src).expect("compile_and_run") {
        Observation::Value(v) => {
            assert_eq!(v.to_uppercase(), "DONE");
        }
        other => panic!("expected Value DONE, got {other:?}"),
    }
}

#[test]
fn count_down_zero_is_done() {
    let src = r#"
(def count-down
  (lambda (n)
    (cond ((eq n 0) (quote done))
          (t (count-down (- n 1))))))
(count-down 0)
"#;
    match compile_and_run(src).expect("compile_and_run") {
        Observation::Value(v) => assert_eq!(v.to_uppercase(), "DONE"),
        other => panic!("expected DONE, got {other:?}"),
    }
}
