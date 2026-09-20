use cml::ir::{Ir, PrimOp};
use cml::{lower, parser};
use my_lisp::{eval_program, load_core_library, Session};
use std::fs;
use std::path::PathBuf;

fn observed_current_frontend_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../my-lisp/lib/compiler/cml-bootstrap.lisp");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "observed-current my-lisp frontend must exist at {}: {error}",
            path.display()
        )
    })
}

fn rust_lowering_envelope(source: &str) -> String {
    let parsed = parser::parse(source).expect("CML source must parse");
    let lowered = lower::lower_program(&parsed).expect("CML source must lower");

    let [Ir::Prim {
        op: PrimOp::Add,
        args,
    }] = lowered.as_slice()
    else {
        panic!("bounded bootstrap witness must lower to one Prim(Add): {lowered:?}");
    };

    let [Ir::Int(left), Ir::Int(right)] = args.as_slice() else {
        panic!("bounded bootstrap witness must preserve exact integer operands: {args:?}");
    };

    format!(
        "(cml-ir-bootstrap-v0 (prim + (literal {left}) (literal {right})))"
    )
}

#[test]
fn lisp_authored_frontend_and_existing_cml_lowering_agree_on_bounded_add() {
    let frontend = observed_current_frontend_source();

    let mut session = Session::default();
    load_core_library(&mut session).expect("supported-pin evaluator must load core");
    eval_program(&frontend, &mut session)
        .expect("observed-current Lisp frontend must execute under the supported evaluator");

    let lisp_envelope = eval_program(
        "(cml-bootstrap-lower-add (quote (+ 1 2)))",
        &mut session,
    )
    .expect("Lisp-authored frontend witness must execute")
    .value
    .to_string();

    let rust_envelope = rust_lowering_envelope("(+ 1 2)");

    assert_eq!(
        rust_envelope, lisp_envelope,
        "CML lowering must agree with the Lisp-authored compiler envelope"
    );
}

#[test]
fn cml_consumes_frontend_without_importing_sid_or_machine_authority() {
    let frontend = observed_current_frontend_source();
    assert!(!frontend.contains("semantic-id"));
    assert!(!frontend.contains("x86-encode"));
    assert!(!frontend.contains("machine-op"));
    assert!(frontend.contains("compiler-frontend-rejection"));
}
