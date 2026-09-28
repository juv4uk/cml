use cml::{
    lower, parser,
    witness_bridge::{
        X86InputValue, execute_x86_actual_with_metadata,
        execute_x86_actuals_with_metadata_and_inputs,
    },
    x86_freestanding::X86FreestandingBackend,
};

#[test]
fn typed_sid8_round_trips_through_native_witness_without_text_alias() {
    let source = "(def BOOTSTRAP-ENTRY (lambda (FORM) FORM))";
    let expressions = parser::parse(source).expect("identity source must parse");
    let program = lower::lower_program(&expressions).expect("identity source must lower");
    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata_and_input_entry(&program, "BOOTSTRAP-ENTRY")
        .expect("identity input-entry must compile");

    let actuals = execute_x86_actuals_with_metadata_and_inputs(
        &compiled,
        &[X86InputValue::sid8(0b0000_0101)],
    )
    .expect("typed SID8 must cross target transport");

    assert_eq!(actuals, vec!["(value \"00000101\")"]);
}

#[test]
fn standalone_sid8_value_materializes_through_target_constructor() {
    let source = "(def SID-VALUE (lambda () 00000101))\n(SID-VALUE)";
    let expressions = parser::parse(source).expect("standalone SID8 source must parse");
    let program = lower::lower_program(&expressions).expect("standalone SID8 source must lower");
    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata(&program)
        .expect("standalone SID8 value must compile through target constructor");

    assert!(
        compiled.assembly.contains("call wsm_sid8_new"),
        "x86 backend must materialize SID8 through target ABI, never a text/fixnum alias"
    );
    let actual = execute_x86_actual_with_metadata(&compiled)
        .expect("standalone SID8 native observation must execute");
    assert_eq!(actual, "(value \"00000101\")");
}
