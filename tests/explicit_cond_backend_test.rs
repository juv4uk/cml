use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::c_backend::CBackend;
use cml::compiler::Compiler;
use cml::ir::Ir;
use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;

const MATCH_SOURCE: &str = r#"
    (cond
      ((quote (identity-relation distinct)) (identity-relation same)
       (quote wrong))
      ((quote (identity-relation same)) (identity-relation same)
       (quote matched)))
"#;

fn lower_match_source() -> Vec<Ir> {
    let expressions = parser::parse(MATCH_SOURCE).unwrap();
    lower::lower_program(&expressions).unwrap()
}

#[test]
fn c_backend_executes_canonical_cond_by_private_structural_match() {
    let program = lower_match_source();
    let c_source = CBackend::new().compile_program(&program).unwrap();
    assert!(c_source.contains("v_equal_p("));

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-cond-match-{}-{nonce}", std::process::id()));
    let source = base.with_extension("c");
    let binary = base.with_extension("bin");
    fs::write(&source, &c_source).unwrap();

    let compile = Command::new("gcc")
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "gcc failed: {}\n--- generated C ---\n{c_source}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&binary).output().unwrap();
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(binary);
    assert!(run.status.success());
    assert_eq!(String::from_utf8(run.stdout).unwrap().trim(), "MATCHED");
}

#[test]
fn fpga_backend_routes_canonical_cond_through_private_cml_equal() {
    let program = lower_match_source();
    let assembly = Compiler::new().compile(&program).unwrap();
    assert!(assembly.contains("CALL R14 cml_equal"));
    assert!(assembly.contains("cml_equal:"));
    assert!(assembly.contains("cond_match_next_"));
}

#[test]
fn x86_freestanding_rejects_canonical_cond_until_private_matcher_exists() {
    let program = lower_match_source();
    let error = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect_err("x86 must fail closed instead of falling back to historical truthiness");
    assert_eq!(
        error.to_string(),
        "unsupported IR variant: CondMatch (explicit result matcher not yet implemented)"
    );
}

#[test]
fn tail_call_lowering_preserves_canonical_match_and_marks_only_the_body_call() {
    let source = r#"
        (def walk
          (lambda (xs)
            (cond
              ((quote continue) continue
               (walk (cdr xs))))))
    "#;
    let expressions = parser::parse(source).unwrap();
    let program = lower::lower_program_with_tail_calls(&expressions).unwrap();
    let [Ir::Def { value, .. }] = program.as_slice() else {
        panic!("expected one lowered definition");
    };
    let Ir::Lambda { body, .. } = value.as_ref() else {
        panic!("expected definition value to remain a lambda");
    };
    let Ir::CondMatch { branches } = body.as_ref() else {
        panic!("canonical cond must remain explicit-match control after tail-call marking");
    };
    let [(query, _expected, branch_body)] = branches.as_slice() else {
        panic!("expected one canonical branch");
    };
    assert!(matches!(query, Ir::Quote(_)));
    assert!(matches!(branch_body, Ir::TailSelfCall { .. }));
}
