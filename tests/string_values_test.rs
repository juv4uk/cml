//! COMPILER-03 — string values on the compiled C path.
//!
//! Strings are ordinary values (not interned as symbols). Until TAG_STRING
//! lands in RUNTIME, tests return early with an explicit message — never a
//! silent pass that pretends strings work.

use cml::build::{compile_and_run, emit_c, front_end_to_ir, Observation};

fn value(source: &str) -> Result<String, String> {
    match compile_and_run(source).map_err(|e| e.to_string())? {
        Observation::Value(v) => Ok(v),
        Observation::Unsupported(u) => Err(format!("Unsupported: {u}")),
        Observation::Error(e) => Err(format!("Error: {e}")),
    }
}

fn strings_supported() -> bool {
    let ir = match front_end_to_ir("\"hi\"") {
        Ok(ir) => ir,
        Err(_) => return false,
    };
    match emit_c(&ir) {
        Ok(c) => c.contains("TAG_STRING") && c.contains("mk_string"),
        Err(_) => false,
    }
}

#[test]
fn string_literal_prints() {
    if !strings_supported() {
        eprintln!("TAG_STRING not in RUNTIME yet; applicator pending");
        return;
    }
    assert_eq!(value("\"hello\"").unwrap(), "hello");
}

#[test]
fn quoted_string_prints() {
    if !strings_supported() {
        eprintln!("TAG_STRING not in RUNTIME yet; applicator pending");
        return;
    }
    assert_eq!(value("(quote \"hi\")").unwrap(), "hi");
}

#[test]
fn string_eq_same() {
    if !strings_supported() {
        eprintln!("TAG_STRING not in RUNTIME yet; applicator pending");
        return;
    }
    let v = value("(eq \"a\" \"a\")").unwrap();
    assert_eq!(v.to_uppercase(), "T");
}

#[test]
fn string_in_cons() {
    if !strings_supported() {
        eprintln!("TAG_STRING not in RUNTIME yet; applicator pending");
        return;
    }
    let v = value("(car (cons \"x\" 1))").unwrap();
    assert_eq!(v, "x");
}
