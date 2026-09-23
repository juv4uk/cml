use std::fs;

use cml::{
    lower, parser, witness_bridge::execute_x86_actual_with_metadata,
    x86_freestanding::X86FreestandingBackend,
};

const EXPECTED_S2_IR: &str = "(value \"(prim cons ((quote A) (quote B)))\")";

#[test]
fn core1_compiled_emitter_executes_bounded_compiler_form_natively() {
    let prelude_path = std::env::var("WSM_MY_LISP_CORE1_PRELUDE_SOURCE")
        .expect("focused S3a witness requires WSM_MY_LISP_CORE1_PRELUDE_SOURCE");
    let compiler_path = std::env::var("WSM_MY_LISP_COMPILER_SOURCE")
        .expect("focused S3a witness requires WSM_MY_LISP_COMPILER_SOURCE");

    let prelude = fs::read_to_string(&prelude_path).unwrap_or_else(|error| {
        panic!("read pinned Core1 compiler prelude {prelude_path}: {error}")
    });
    let compiler = fs::read_to_string(&compiler_path)
        .unwrap_or_else(|error| panic!("read pinned S2 compiler source {compiler_path}: {error}"));

    let source =
        format!("{prelude}\n{compiler}\n(compiler-form (quote (cons (quote A) (quote B))))\n");

    let expressions = parser::parse(&source)
        .unwrap_or_else(|error| panic!("S3a combined Core1 source must parse: {error:?}"));
    let program = lower::lower_program(&expressions)
        .unwrap_or_else(|error| panic!("S3a combined Core1 source must lower: {error}"));

    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata(&program)
        .unwrap_or_else(|error| panic!("S3a Core1 emitter must compile natively: {error}"));

    let actual = execute_x86_actual_with_metadata(&compiled)
        .unwrap_or_else(|error| panic!("S3a native emitter observation failed: {error}"));

    assert_eq!(
        actual, EXPECTED_S2_IR,
        "native compiled emitter must reproduce the already-admitted S2 compiler-form product"
    );
}
