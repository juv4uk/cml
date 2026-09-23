use cml::{
    lower, parser,
    x86_freestanding::X86FreestandingBackend,
};

#[test]
fn input_entry_metadata_keeps_explicit_quoted_transport_vocabulary() {
    let source = r#"
        (def bootstrap-entry (lambda (input) input))
        (quote car)
        (quote cons)
        (quote quote)
        (quote A)
        (quote B)
    "#;

    let expressions = parser::parse(source).expect("probe source must parse");
    let program = lower::lower_program(&expressions).expect("probe source must lower");
    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata_and_input_entry(&program, "BOOTSTRAP-ENTRY")
        .expect("input-entry metadata probe must compile");

    let names: Vec<&str> = compiled.symbols.iter().map(|symbol| symbol.name.as_str()).collect();
    eprintln!("S3b input-entry metadata names = {names:?}");

    for required in ["car", "cons", "quote", "A", "B"] {
        assert!(
            names.contains(&required),
            "input-entry metadata must retain explicit quoted transport symbol {required:?}; actual={names:?}"
        );
    }
}
