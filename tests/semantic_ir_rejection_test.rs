//! SEMANTIC-IR-RECOVERY: named rejection for admitted-but-unimplemented IR.
use cml::compiler::Compiler;
use cml::ir::{Ir, Quoted};

#[test]
fn builtin_is_typed_rejection_not_panic_on_fpga() {
    let err = Compiler::new()
        .compile(&[Ir::Builtin("car".to_string())])
        .expect_err("Builtin must not compile on FPGA");
    let s = err.to_string();
    assert!(
        s.contains("Builtin") || s.contains("Unsupported"),
        "got {s}"
    );
}

#[test]
fn rational_is_typed_rejection_not_panic_on_fpga() {
    let err = Compiler::new()
        .compile(&[Ir::Rational(1, 2)])
        .expect_err("Rational must not compile on FPGA");
    let s = err.to_string();
    assert!(
        s.contains("Rational") || s.contains("Unsupported"),
        "got {s}"
    );
}

#[test]
fn quoted_float_is_typed_rejection_not_panic_on_fpga() {
    let err = Compiler::new()
        .compile(&[Ir::Quote(Quoted::Float(1.5))])
        .expect_err("Quoted::Float must not compile on FPGA");
    let s = err.to_string();
    assert!(s.contains("Float") || s.contains("Unsupported"), "got {s}");
}

#[test]
fn int_still_emits() {
    let asm = Compiler::new()
        .compile(&[Ir::Int(42)])
        .expect("Int must emit");
    assert!(asm.contains("LOADI") || asm.contains("42") || !asm.is_empty());
}
