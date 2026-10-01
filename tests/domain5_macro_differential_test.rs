//! #408 — D5 compiler falsifier.
//!
//! Differentially compares CML's live Rust MacroExpander with the repository's
//! Lisp-hosted macros.lisp implementation on staging-sensitive fixtures.
//!
//! This is a research witness, not a new compiler authority.

use cml::ast::{Expr, NumericBufferLiteral};
use cml::macros::MacroExpander;
use cml::parser;
use sens::{Session, eval_program, load_core_library};

fn render_expr(expr: &Expr) -> String {
    match expr {
        Expr::Sid(sid) => sid.to_string(),
        Expr::Integer(value) => value.to_string(),
        Expr::Rational(num, den) => format!("{num}/{den}"),
        Expr::Symbol(symbol) => symbol.clone(),
        Expr::String(value) => format!("{value:?}"),
        Expr::List(items) => {
            let inner = items.iter().map(render_expr).collect::<Vec<_>>().join(" ");
            format!("({inner})")
        }
        Expr::DottedList(items, tail) => {
            let mut rendered = items.iter().map(render_expr).collect::<Vec<_>>();
            rendered.push(".".to_string());
            rendered.push(render_expr(tail));
            format!("({})", rendered.join(" "))
        }
        Expr::NumericBuffer(NumericBufferLiteral::I32(values)) => {
            let inner = values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            format!("#i32({inner})")
        }
        Expr::NumericBuffer(NumericBufferLiteral::F32(bits)) => {
            let inner = bits
                .iter()
                .map(|bits| f32::from_bits(*bits).to_string())
                .collect::<Vec<_>>()
                .join(" ");
            format!("#f32({inner})")
        }
    }
}

fn render_program(exprs: &[Expr]) -> String {
    let inner = exprs.iter().map(render_expr).collect::<Vec<_>>().join(" ");
    format!("({inner})")
}

fn lisp_macro_session() -> Session {
    let mut session = Session::default();
    load_core_library(&mut session).expect("pinned SENS core must load");

    let source = include_str!("../macros.lisp");
    eval_program(source, &mut session)
        .expect("cml/macros.lisp must load in the pinned SENS interpreter");

    session
}

fn rust_expand(source: &str) -> String {
    let parsed = parser::parse(source).expect("CML source must parse");
    let expanded = MacroExpander::new()
        .process(&parsed)
        .expect("Rust MacroExpander must expand fixture");
    render_program(&expanded)
}

fn lisp_expand(session: &mut Session, source: &str) -> String {
    // Parse through CML first, then render the exact CML AST as quoted data.
    // This prevents reader differences from masquerading as macro-semantic
    // differences in this differential witness.
    let parsed = parser::parse(source).expect("CML source must parse");
    let program_data = render_program(&parsed);
    let call = format!("(expand-program (quote {program_data}))");
    eval_program(&call, session)
        .unwrap_or_else(|error| panic!("Lisp macro expansion failed for {source:?}: {error:?}"))
        .value
        .to_string()
}

fn assert_differential(session: &mut Session, name: &str, source: &str, expected: &str) {
    let rust = rust_expand(source);
    let lisp = lisp_expand(session, source);

    assert_eq!(
        rust, expected,
        "{name}: Rust expansion changed unexpectedly"
    );
    assert_eq!(
        lisp, expected,
        "{name}: Lisp-hosted expansion changed unexpectedly"
    );
    assert_eq!(
        rust, lisp,
        "{name}: Rust MacroExpander and macros.lisp disagree"
    );
}


#[test]
fn lisp_macro_reference_uses_registry_admitted_predicate_surfaces() {
    let operations = include_str!("../contracts/cml-operations.lisp");
    let macros = include_str!("../macros.lisp");

    assert!(operations.contains("(canonical-name . \"atom?\")"));
    assert!(operations.contains("(canonical-name . \"eq?\")"));

    assert!(
        !macros.contains("(atom "),
        "macros.lisp must not execute retired/unadmitted atom surface"
    );
    assert!(
        !macros.contains("(eq "),
        "macros.lisp must not execute retired/unadmitted eq surface"
    );

    assert!(
        macros.contains("(quote atom)"),
        "CML macro meta-language must still recognize historical atom operator syntax as data"
    );
    assert!(
        macros.contains("(quote eq)"),
        "CML macro meta-language must still recognize historical eq operator syntax as data"
    );
}

#[test]
fn d5_one_transformer_contract_matches_rust_and_lisp_macro_authorities() {
    let mut session = lisp_macro_session();

    let corpus = [
        (
            "variadic-list",
            "(defmacro my-list items (cons (quote quote) (cons items (quote ())))) (my-list 1 2 3)",
            "((quote (1 2 3)))",
        ),
        (
            "raw-unused-operand",
            "(defmacro first-form (a b) a) (first-form (quote ok) never-defined)",
            "((quote ok))",
        ),
        (
            "nested-transformer",
            "(defmacro inner (x) x) (defmacro outer (x) (cons (quote inner) (cons x (quote ())))) (outer (quote ok))",
            "((quote ok))",
        ),
        (
            "conditional-builder",
            "(defmacro my-if (test then else) (cons (quote cond) (cons (cons test (cons then (quote ()))) (cons (cons (quote t) (cons else (quote ()))) (quote ()))))) (my-if (eq 1 1) 42 0)",
            "((cond ((eq 1 1) 42) (t 0)))",
        ),
        (
            "forward-definition-not-retroactive",
            "(later 1) (defmacro later (x) x)",
            "((later 1))",
        ),
    ];

    for (name, source, expected) in corpus {
        assert_differential(&mut session, name, source, expected);
    }
}

#[test]
fn compiler_macro_stage_is_behaviorally_reproducible_by_lisp_hosted_transformer_logic() {
    let source =
        "(defmacro my-list items (cons (quote quote) (cons items (quote ())))) (my-list 4 5)";
    let mut session = lisp_macro_session();

    let rust = rust_expand(source);
    let lisp = lisp_expand(&mut session, source);

    assert_eq!(rust, lisp);
    assert_eq!(rust, "((quote (4 5)))");

    // This test intentionally proves parity, not wiring. The live compiler can
    // still use Rust MacroExpander until a separate migration removes duplicate
    // authority, but parity means Rust expansion is not evidence for a second
    // language-level D5 identity by itself.
}
