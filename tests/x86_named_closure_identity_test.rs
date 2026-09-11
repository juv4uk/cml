use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use cml::{lower, parser, x86_freestanding::X86FreestandingBackend};

/// Stage2 pressure witness copied from the *shape* of current meta-eval
/// provenance: a top-level unary function is used as an identity token,
/// stored inside data, then compared later with `eq`.
///
/// The important property is not merely "a bare def name compiles". Re-reading
/// the same top-level binding must yield the SAME closure identity within one
/// program run; allocating a fresh descriptor at each reference would make the
/// second `eq` false and would silently break `my-result-fail?` semantics.
#[test]
fn named_unary_function_is_a_stable_first_class_closure_identity() {
    let source = r#"
        (def ok-token (lambda (value) value))
        (def make-ok (lambda (value) (cons ok-token value)))
        (def ok? (lambda (result) (eq (car result) ok-token)))
        (ok? (make-ok 42))
    "#;

    let expressions = parser::parse(source).expect("Stage2 token fixture must parse");
    let program = lower::lower_program(&expressions).expect("Stage2 token fixture must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("a unary top-level def used as data must materialize as a stable closure");

    assert!(
        assembly.contains("call wsm_closure_new"),
        "named token must be backed by the ratified closure ABI"
    );
    assert!(
        assembly.contains(".Lnamed_closure_word_"),
        "named token identity must be stored once and reloaded, not reallocated per reference"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock must be after epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-stage2-named-closure-{}-{nonce}",
        std::process::id()
    ));
    let asm_path = base.with_extension("s");
    let c_path = base.with_extension("c");
    let exe_path = base.with_extension("bin");

    fs::write(&asm_path, assembly).expect("write generated assembly");
    fs::write(
        &c_path,
        format!(
            "#include <stdint.h>\nextern uint64_t wsm_entry(void *);\nint main(void) {{ return wsm_entry(0) == UINT64_C({}) ? 0 : 1; }}\n",
            wsm_os_target::CANONICAL_T
        ),
    )
    .expect("write C witness harness");

    let linked = Command::new("cc")
        .arg(&c_path)
        .arg(&asm_path)
        .arg("/home/agents/GitHub/wsm-my-lisp/asm/nucleus.s")
        .arg("-o")
        .arg(&exe_path)
        .output()
        .expect("cc must be available for Stage2 closure witness");
    assert!(
        linked.status.success(),
        "Stage2 named-closure witness must link: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = Command::new(&exe_path)
        .output()
        .expect("compiled Stage2 named-closure witness must execute");

    let _ = fs::remove_file(&asm_path);
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(&exe_path);

    assert!(
        run.status.success(),
        "meta-eval-shaped token identity must survive store/reload and eq; stdout={} stderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
