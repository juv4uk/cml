use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;

#[test]
fn encoder_low3_definition_compiles_through_x86_freestanding() {
    // Minimal source-derived slice from upstream
    // my-lisp/lib/machine/encoding/x86-64.lisp.
    //
    // This is intentionally not a semantic expected-value oracle. It asks only
    // whether CML can admit the encoder helper's own function application.
    let source = r#"
        (def x86-low3
          (lambda (code)
            (mod code 8)))
    "#;

    let expressions = parser::parse(source).expect("encoder-derived slice must parse");
    let program = lower::lower_program(&expressions).expect("encoder-derived slice must lower");

    X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("x86-low3 encoder helper must compile");
}
