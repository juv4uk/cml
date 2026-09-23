use std::collections::BTreeSet;
use std::fs;

use cml::{
    ast::Expr,
    lower, parser,
    witness_bridge::{X86InputValue, execute_x86_actuals_with_metadata_and_inputs},
    x86_freestanding::X86FreestandingBackend,
};

fn collect_symbols(expr: &Expr, out: &mut BTreeSet<String>) {
    match expr {
        Expr::Symbol(name) => {
            out.insert(name.clone());
        }
        Expr::List(items) => {
            for item in items {
                collect_symbols(item, out);
            }
        }
        Expr::DottedList(items, tail) => {
            for item in items {
                collect_symbols(item, out);
            }
            collect_symbols(tail, out);
        }
        Expr::Sid(_)
        | Expr::Integer(_)
        | Expr::Rational(_, _)
        | Expr::String(_)
        | Expr::NumericBuffer(_) => {}
    }
}

fn symbol_word(compiled: &cml::x86_freestanding_metadata::X86CompiledProgram, name: &str) -> u64 {
    compiled
        .symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| panic!("S4 transport vocabulary must contain symbol {name:?}"))
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

fn encode_expr(
    expr: &Expr,
    compiled: &cml::x86_freestanding_metadata::X86CompiledProgram,
) -> X86InputValue {
    match expr {
        Expr::Sid(_) => {
            panic!(
                "S4 target input transport has no admitted SID8 ABI yet; do not alias SID8 through a symbol/string/integer word"
            )
        }
        Expr::Integer(value) => X86InputValue::word(
            wsm_os_target::encode_fixnum(*value)
                .unwrap_or_else(|| panic!("fixture integer outside target fixnum range: {value}")),
        ),
        Expr::Symbol(name) => X86InputValue::word(symbol_word(compiled, name)),
        Expr::List(items) => list(
            items
                .iter()
                .map(|item| encode_expr(item, compiled))
                .collect(),
        ),
        Expr::DottedList(items, tail) => items
            .iter()
            .rev()
            .fold(encode_expr(tail, compiled), |cdr, car| {
                X86InputValue::cons(encode_expr(car, compiled), cdr)
            }),
        Expr::Rational(_, _) | Expr::String(_) | Expr::NumericBuffer(_) => {
            panic!("S4 Core1 compiler source fixture must use only target-data AST nodes")
        }
    }
}

fn compiler_named_def<'a>(forms: &'a [Expr], name: &str) -> &'a Expr {
    forms
        .iter()
        .find(|expr| {
            matches!(
                expr,
                Expr::List(items)
                    if items.len() >= 3
                    && items[0].is_symbol("def")
                    && items[1].is_symbol(name)
            )
        })
        .unwrap_or_else(|| panic!("pinned compiler source must contain def {name}"))
}

#[test]
fn s4_compiled_emitter_compiles_its_real_core1_cond_definition() {
    let prelude_path = std::env::var("WSM_MY_LISP_CORE1_PRELUDE_SOURCE")
        .expect("S4 witness requires WSM_MY_LISP_CORE1_PRELUDE_SOURCE");
    let compiler_path = std::env::var("WSM_MY_LISP_COMPILER_SOURCE")
        .expect("S4 witness requires WSM_MY_LISP_COMPILER_SOURCE");

    let prelude = fs::read_to_string(&prelude_path)
        .unwrap_or_else(|error| panic!("read pinned Core1 compiler prelude: {error}"));
    let compiler = fs::read_to_string(&compiler_path)
        .unwrap_or_else(|error| panic!("read pinned S2 compiler source: {error}"));
    let compiler_forms = parser::parse(&compiler).expect("pinned S2 compiler source must parse");

    // Use a real self-source definition with historical two-part COND.
    let input_form = compiler_named_def(&compiler_forms, "compiler-nil?").clone();

    // Transport vocabulary only: ensure every symbol in the exact source form
    // has an image-local target word. These inert quote rows are never the S4
    // input and do not evaluate or reinterpret the compiler source.
    let mut vocabulary = BTreeSet::new();
    collect_symbols(&input_form, &mut vocabulary);
    let vocabulary_source = vocabulary
        .iter()
        .map(|name| format!("(quote {name})"))
        .collect::<Vec<_>>()
        .join("\n");

    let source = format!(
        "{prelude}\n{compiler}\n\
         (def bootstrap-entry (lambda (input) (compiler-form input)))\n\
         {vocabulary_source}\n"
    );
    let expressions = parser::parse(&source).expect("S4 combined source must parse");
    let program = lower::lower_program(&expressions).expect("S4 combined source must lower");
    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata_and_input_entry(&program, "BOOTSTRAP-ENTRY")
        .expect("S4 compiled emitter must build with explicit input entry");

    let input = encode_expr(&input_form, &compiled);
    let actual = execute_x86_actuals_with_metadata_and_inputs(&compiled, &[input])
        .expect("S4 compiled emitter must process its own real compiler source");

    // Core1 source owns historical two-part COND. A Core1 self-compilation
    // must preserve that profile rather than upgrading it into Contract-8
    // three-field cond-match clauses.
    let expected = concat!(
        "(value \"(def compiler-nil? ",
        "(lambda (form) ",
        "(cond ",
        "((prim atom ((var form))) (prim eq ((var form) (quote ())))) ",
        "((var t) (quote ())))))\")"
    );

    assert_eq!(
        actual,
        vec![expected.to_string()],
        "S4 Gen1 cannot advance until the native compiler can compile a real Core1 two-part-COND definition from its own pinned source"
    );
}
