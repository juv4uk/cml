//! End-to-end: my-lisp semantic export slice-1 program shape through CML C path.
//!
//! Fixture #69 shape (named def + recursion):
//!   (def count-down (lambda (n) (cond ((eq n 0) (quote done)) (t (count-down (- n 1))))))
//!   (count-down N)
//!
//! Forms covered by export: quote, cond, lambda, define, eq, subtraction.

use cml::build::{Observation, compile_and_run};
use cml::semantic_export::{check_digest, parse_export, validate_slice1};

const EXPORT: &str = include_str!("../contracts/mylisp-cml-export.wsm");

/// Real producer digest (my-lisp `cml-export` binary output), vendored into
/// `contracts/mylisp-cml-export.wsm` -- issue cml#3 item 1. This is a
/// separate constant, not read off the file itself, so a drifted vendor
/// copy actually fails this test instead of trivially agreeing with itself.
const EXPECTED_PRODUCER_DIGEST: &str = "dfc880e5e5ae80f9";

#[test]
fn export_slice1_validates() {
    let export = parse_export(EXPORT).expect("parse export");
    validate_slice1(&export).expect("validate slice1");
}

#[test]
fn export_digest_matches_pinned_producer_digest() {
    let export = parse_export(EXPORT).expect("parse export");
    check_digest(&export, EXPECTED_PRODUCER_DIGEST)
        .expect("digest must match the pinned producer run");
}

#[test]
fn export_digest_check_fails_closed_on_drift() {
    let export = parse_export(EXPORT).expect("parse export");
    let result = check_digest(&export, "some-other-digest-entirely");
    assert!(
        result.is_err(),
        "check_digest must fail closed on a real mismatch, not silently pass"
    );
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
