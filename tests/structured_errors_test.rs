//! COMPILER-05 — structured failure identity on the compiled path.
//!
//! Runtime must print stable "Kind: detail" lines for admitted failures.
//! Classification goes through build::classify_runtime_stderr / compile_and_run.

use cml::build::{compile_and_run, Observation};
use cml::runtime_abi::ERROR_KINDS;

fn err(source: &str) -> String {
    match compile_and_run(source).expect("compile_and_run") {
        Observation::Error(e) => e,
        other => panic!("expected Error, got {other:?} for {source:?}"),
    }
}

#[test]
fn division_by_zero_kind() {
    let e = err("(/ 1 0)");
    assert!(
        e.starts_with("DivisionByZero"),
        "expected DivisionByZero prefix, got {e}"
    );
}

#[test]
fn unknown_symbol_kind() {
    let e = err("no-such-var");
    assert!(
        e.starts_with("UnknownSymbol"),
        "expected UnknownSymbol prefix, got {e}"
    );
}

#[test]
fn arity_kind_on_cons() {
    let e = err("(cons 1)");
    assert!(
        e.starts_with("Arity") || e.contains("Arity"),
        "expected Arity, got {e}"
    );
}

#[test]
fn type_kind_on_car_of_atom() {
    let e = err("(car 1)");
    assert!(
        e.starts_with("Type") || e.contains("Type"),
        "expected Type, got {e}"
    );
}

#[test]
fn abi_error_kinds_are_nonempty() {
    assert!(ERROR_KINDS.contains(&"DivisionByZero"));
    assert!(ERROR_KINDS.contains(&"NumericOverflow"));
    assert!(ERROR_KINDS.contains(&"Arity"));
}
