use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// wsm-my-lisp#34 / cml#154
///
/// This is deliberately the smallest runtime-callable witness for the real
/// Stage2 meta-evaluator dependency. The callee inside `apply1` is not a
/// statically named function: `fn` is a lexical value holding the closure
/// produced for `id`.
///
/// Direct named application is already supported. Passing `id` as a value and
/// then executing `(fn arg)` is the missing capability needed by the real
/// meta-eval call graph. Do not replace this with a direct call to `id`;
/// that would turn this back into an already-proven capability.
#[test]
fn runtime_unary_closure_value_application_executes() {
    let source = r#"
        (def apply1 (lambda (fn arg) (fn arg)))
        (def id (lambda (x) x))
        (apply1 id 42)
    "#;

    let expressions = parser::parse(source).expect("runtime-closure witness must parse");
    let program = lower::lower_program(&expressions).expect("runtime-closure witness must lower");

    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("bounded unary closure value application must compile");

    // The implementation must stay on the existing closure ABI rather than
    // inventing a second callable representation.
    assert!(
        assembly.contains("wsm_closure_definition"),
        "runtime closure dispatch must inspect the existing closure descriptor"
    );

    let expected = wsm_os_target::encode_fixnum(42).expect("42 must fit target fixnum");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-runtime-closure-application-{}-{nonce}",
        std::process::id()
    ));
    let asm = base.with_extension("s");
    let harness = base.with_extension("c");
    let executable = base.with_extension("bin");

    fs::write(&asm, assembly).unwrap();
    fs::write(
        &harness,
        format!(
            "#include <stdint.h>\n#include <stdlib.h>\nextern uint64_t wsm_entry(void *);\nint main(void) {{ return wsm_entry(0) == {expected}ULL ? 0 : 1; }}\n"
        ),
    )
    .unwrap();

    let nucleus = cml::x86_freestanding::resolve_nucleus_asm_path()
        .expect("runtime closure witness must resolve the WSM asm nucleus");
    let linked = Command::new("cc")
        .arg(&harness)
        .arg(&asm)
        .arg(&nucleus)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("cc must execute");

    assert!(
        linked.status.success(),
        "runtime closure witness must link: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("runtime closure witness must execute");

    let _ = fs::remove_file(asm);
    let _ = fs::remove_file(harness);
    let _ = fs::remove_file(executable);

    assert!(
        run.status.success(),
        "(apply1 id 42) must execute through the runtime closure value and return 42"
    );
}
