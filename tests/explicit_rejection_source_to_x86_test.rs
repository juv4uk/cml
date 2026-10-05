use cml::x86_freestanding::{CompileError, X86FreestandingBackend};
use cml::{lower, parser};

fn assert_explicit_rejection(source: &str, expected_reason: &'static str) {
    let parsed = parser::parse(source).unwrap_or_else(|error| {
        panic!("source must parse before target rejection: {source}: {error:?}")
    });
    let lowered = lower::lower_program(&parsed).unwrap_or_else(|error| {
        panic!("source must lower before target rejection: {source}: {error:?}")
    });

    assert_eq!(
        lowered.len(),
        1,
        "unsupported source form must survive lowering as exactly one IR request: {source}"
    );

    let error = X86FreestandingBackend::new()
        .compile_program(&lowered)
        .expect_err("unsupported target form must fail closed instead of emitting an artifact");

    match error {
        CompileError::UnsupportedVariant(reason) => assert_eq!(
            reason, expected_reason,
            "target rejection must remain named and inspectable for {source}"
        ),
        other => panic!("expected typed UnsupportedVariant for {source}, got {other:?}"),
    }
}

#[test]
fn known_but_unsupported_source_forms_never_disappear_on_x86() {
    assert_explicit_rejection("0.5", "Rational");
    assert_explicit_rejection(r#""hello""#, "String");
    // exact-Q `<` is now an admitted x86 mechanism; keep this corpus on genuinely unsupported classes.
}
