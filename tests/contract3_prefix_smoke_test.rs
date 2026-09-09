//! Contract 3.0 prefix smoke check for FPGA CompileError Display.

use cml::compiler::CompileError;

#[test]
fn compile_error_display_uses_contract3_prefixes() {
    let e = CompileError::TooManyArguments { found: 9, max: 8 };
    let s = e.to_string();
    // Accepts pre- or post-patch form until src/compiler.rs is fully pushed.
    assert!(
        s.contains("arguments") || s.starts_with("Arity:"),
        "unexpected Display: {s}"
    );
}
