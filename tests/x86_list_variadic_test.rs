use cml::{
    lower,
    parser,
    witness_bridge::execute_x86_actual_with_metadata,
    x86_freestanding::X86FreestandingBackend,
};

fn run(source: &str) -> String {
    let expressions = parser::parse(source).expect("fixture must parse");
    let program = lower::lower_program(&expressions).expect("fixture must lower");
    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata(&program)
        .expect("fixture must compile to native x86");
    execute_x86_actual_with_metadata(&compiled).expect("native actual must decode")
}

#[test]
fn zero_arity_list_returns_nil_natively() {
    assert_eq!(run("(list)"), "(value \"()\")");
}

#[test]
fn nary_list_preserves_argument_order_and_tail_natively() {
    assert_eq!(
        run("(list (quote A) (quote B) (quote C))"),
        "(value \"(A B C)\")"
    );
}
