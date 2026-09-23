use std::fs;

use cml::{
    lower, parser,
    witness_bridge::{X86InputValue, execute_x86_actuals_with_metadata_and_inputs},
    x86_freestanding::X86FreestandingBackend,
};

fn symbol_word(compiled: &cml::x86_freestanding_metadata::X86CompiledProgram, name: &str) -> u64 {
    compiled
        .symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| panic!("compiler metadata must contain symbol {name:?}"))
        .encoded_word
}

fn list(items: Vec<X86InputValue>) -> X86InputValue {
    items
        .into_iter()
        .rev()
        .fold(X86InputValue::word(wsm_os_target::NIL), |cdr, car| {
            X86InputValue::cons(car, cdr)
        })
}

#[test]
fn core1_compiled_emitter_accepts_two_composite_inputs_without_recompile() {
    let prelude_path = std::env::var("WSM_MY_LISP_CORE1_PRELUDE_SOURCE")
        .expect("focused S3b witness requires WSM_MY_LISP_CORE1_PRELUDE_SOURCE");
    let compiler_path = std::env::var("WSM_MY_LISP_COMPILER_SOURCE")
        .expect("focused S3b witness requires WSM_MY_LISP_COMPILER_SOURCE");

    let prelude = fs::read_to_string(&prelude_path)
        .unwrap_or_else(|error| panic!("read pinned Core1 compiler prelude: {error}"));
    let compiler = fs::read_to_string(&compiler_path)
        .unwrap_or_else(|error| panic!("read pinned S2 compiler source: {error}"));

    let s3a_control_source = format!(
        "{prelude}\n{compiler}\n\
         (compiler-form (quote (cons (quote A) (quote B))))\n"
    );
    let s3a_control_expressions =
        parser::parse(&s3a_control_source).expect("S3a control source must parse");
    let s3a_control_program =
        lower::lower_program(&s3a_control_expressions).expect("S3a control source must lower");
    X86FreestandingBackend::new()
        .compile_program(&s3a_control_program)
        .expect("current master must preserve the already-proven S3a compile boundary");

    let wrapper_source = format!(
        "{prelude}\n{compiler}\n\
         (def bootstrap-entry (lambda (input) (compiler-form input)))\n"
    );
    let wrapper_expressions =
        parser::parse(&wrapper_source).expect("S3b wrapper source must parse");
    let wrapper_program =
        lower::lower_program(&wrapper_expressions).expect("S3b wrapper source must lower");
    X86FreestandingBackend::new()
        .compile_program_with_input_entry(&wrapper_program, "BOOTSTRAP-ENTRY")
        .expect("S3b wrapper alone must compile before metadata enrichment");

    // Input-vocabulary symbols are inert quoted values used only to make their
    // exact image-local target words part of compiler-owned metadata. The
    // explicit input entry bypasses wsm_entry, so these forms are never
    // evaluated as bootstrap inputs and carry no host-side Lisp semantics.
    let source = format!(
        "{wrapper_source}\
         (quote cons)\n(quote car)\n(quote quote)\n(quote A)\n(quote B)\n"
    );

    let expressions = parser::parse(&source).expect("S3b combined source must parse");
    let program = lower::lower_program(&expressions).expect("S3b combined source must lower");
    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata_and_input_entry(&program, "BOOTSTRAP-ENTRY")
        .expect("metadata-enriched real Core1 emitter must compile with explicit input entry");

    let quote = symbol_word(&compiled, "quote");
    let cons = symbol_word(&compiled, "cons");
    let car = symbol_word(&compiled, "car");
    let a = symbol_word(&compiled, "A");
    let b = symbol_word(&compiled, "B");

    let input_cons = list(vec![
        X86InputValue::word(cons),
        list(vec![X86InputValue::word(quote), X86InputValue::word(a)]),
        list(vec![X86InputValue::word(quote), X86InputValue::word(b)]),
    ]);
    let input_car = list(vec![
        X86InputValue::word(car),
        list(vec![
            X86InputValue::word(quote),
            list(vec![X86InputValue::word(a), X86InputValue::word(b)]),
        ]),
    ]);

    let actual = execute_x86_actuals_with_metadata_and_inputs(&compiled, &[input_cons, input_car])
        .expect("same compiled artifact must process both composite inputs");

    assert_eq!(
        actual,
        vec![
            "(value \"(prim cons ((quote A) (quote B)))\")",
            "(value \"(prim car ((quote (A B))))\")",
        ]
    );
}
