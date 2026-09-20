use cml::ir::{Ir, PrimOp};
use cml::{lower, parser};
use std::fs;
use std::path::PathBuf;

fn observed_current_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../my-lisp")
        .join(relative)
}

fn observed_current_text(relative: &str) -> String {
    let path = observed_current_path(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "observed-current my-lisp artifact must exist at {}: {error}",
            path.display()
        )
    })
}

fn witness_field(name: &str) -> String {
    let source = observed_current_text("tests/fixtures/cml-bootstrap-frontend-witness.lisp");
    let prefix = format!("({name} . \"");
    let start = source
        .find(&prefix)
        .unwrap_or_else(|| panic!("observed-current witness field {name} must exist"))
        + prefix.len();
    let tail = &source[start..];
    let end = tail
        .find("\")")
        .unwrap_or_else(|| panic!("observed-current witness field {name} must be quoted"));
    tail[..end].to_string()
}

fn rust_lowering_envelope(source: &str) -> String {
    let parsed = parser::parse(source).expect("CML source must parse");
    let lowered = lower::lower_program(&parsed).expect("CML source must lower");

    let [
        Ir::Prim {
            op: PrimOp::Add,
            args,
        },
    ] = lowered.as_slice()
    else {
        panic!("bounded bootstrap witness must lower to one Prim(Add): {lowered:?}");
    };

    let [Ir::Int(left), Ir::Int(right)] = args.as_slice() else {
        panic!("bounded bootstrap witness must preserve exact integer operands: {args:?}");
    };

    format!("(cml-ir-bootstrap-v0 (prim + (literal {left}) (literal {right})))")
}

#[test]
fn lisp_owned_frontend_witness_and_existing_cml_lowering_agree_on_bounded_add() {
    let source = witness_field("source");
    let expected_envelope = witness_field("expected-envelope");
    let rust_envelope = rust_lowering_envelope(&source);

    assert_eq!(
        rust_envelope, expected_envelope,
        "CML lowering must agree with the Lisp-owned frontend witness"
    );
}

#[test]
fn cml_consumes_observed_current_witness_without_importing_semantic_authority() {
    let frontend = observed_current_text("lib/compiler/cml-bootstrap.lisp");
    let witness = observed_current_text("tests/fixtures/cml-bootstrap-frontend-witness.lisp");

    assert!(!frontend.contains("semantic-id"));
    assert!(!frontend.contains("x86-encode"));
    assert!(!frontend.contains("machine-op"));
    assert!(frontend.contains("compiler-frontend-rejection"));

    assert!(witness.contains("(authority . \"lib/compiler/cml-bootstrap.lisp\")"));
    assert!(witness.contains("(consumer . \"juv4uk/cml#153\")"));
}
