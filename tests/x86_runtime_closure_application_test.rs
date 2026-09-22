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

/// cml#180: the same runtime-selected callable path must not collapse back to
/// the historical unary-only gate.  The callee is produced as a closure value
/// and applied later with two arguments; a direct lambda call would not prove
/// this capability.
#[test]
fn runtime_binary_closure_value_application_executes() {
    let source = r#"
        (def apply2 (lambda (fn left right) (fn left right)))
        (apply2 (lambda (x y) (+ x y)) 20 22)
    "#;

    let expressions = parser::parse(source).expect("binary runtime-closure witness must parse");
    let program =
        lower::lower_program(&expressions).expect("binary runtime-closure witness must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("bounded binary closure value application must compile");

    assert!(assembly.contains("wsm_closure_definition"));
    assert!(assembly.contains("wsm_closure_environment"));
    assert!(
        assembly.contains("movq %r10"),
        "captured environment must travel separately from the five user-argument registers"
    );

    let expected = wsm_os_target::encode_fixnum(42).expect("42 must fit target fixnum");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-runtime-binary-closure-{}-{nonce}",
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
        .expect("binary runtime closure witness must resolve the WSM asm nucleus");
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
        "binary runtime closure witness must link: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("binary runtime closure witness must execute");

    let _ = fs::remove_file(asm);
    let _ = fs::remove_file(harness);
    let _ = fs::remove_file(executable);

    assert!(
        run.status.success(),
        "(apply2 (lambda (x y) (+ x y)) 20 22) must execute through a runtime closure and return 42"
    );
}

/// cml#180: top-level named functions used as data must use the same bounded
/// fixed-arity closure representation.  This catches a subtle split where
/// anonymous N-ary closures work but named function values remain unary-only.
#[test]
fn runtime_named_binary_function_value_application_executes() {
    let source = r#"
        (def add2 (lambda (x y) (+ x y)))
        (def apply2 (lambda (fn left right) (fn left right)))
        (apply2 add2 20 22)
    "#;

    let expressions = parser::parse(source).expect("named binary callable witness must parse");
    let program =
        lower::lower_program(&expressions).expect("named binary callable witness must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("named binary function value must compile through closure dispatch");

    assert!(assembly.contains(".Lnamed_closure_word_"));
    assert!(assembly.contains("call wsm_closure_new"));
    assert!(assembly.contains("call wsm_closure_definition"));

    let expected = wsm_os_target::encode_fixnum(42).expect("42 must fit target fixnum");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-runtime-named-binary-{}-{nonce}",
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
        .expect("named binary closure witness must resolve the WSM asm nucleus");
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
        "named binary closure witness must link: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("named binary closure witness must execute");

    let _ = fs::remove_file(asm);
    let _ = fs::remove_file(harness);
    let _ = fs::remove_file(executable);

    assert!(
        run.status.success(),
        "(apply2 add2 20 22) must execute through the stable named closure value and return 42"
    );
}

/// The runtime closure object intentionally does not grow an arity field.
/// Instead, the compiler records arity per definition id and emits only
/// matching dispatch arms.  Calling a unary closure through a binary runtime
/// call must therefore take the explicit AbiViolation path, never enter the
/// unary body with a guessed calling convention.
#[test]
fn runtime_closure_arity_mismatch_fails_closed() {
    let source = r#"
        (def id (lambda (x) x))
        (def apply2 (lambda (fn left right) (fn left right)))
        (apply2 id 20 22)
    "#;

    let expressions = parser::parse(source).expect("arity-mismatch witness must parse");
    let program = lower::lower_program(&expressions).expect("arity-mismatch witness must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("runtime arity mismatch is a runtime ABI failure, not a compiler crash");

    assert!(assembly.contains("call wsm_fail"));
    assert_eq!(
        assembly.matches("call .Lclosure_").count(),
        0,
        "a binary dynamic call must not dispatch to the only known unary closure"
    );

    // A tiny ABI harness is enough here and makes the failure observable
    // without depending on the nucleus's process-failure policy.  There is one
    // named closure in this fixture; the harness preserves exactly its
    // definition/environment metadata and exits 77 when the generated code
    // reports AbiViolation.
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-runtime-closure-arity-mismatch-{}-{nonce}",
        std::process::id()
    ));
    let asm = base.with_extension("s");
    let harness = base.with_extension("c");
    let executable = base.with_extension("bin");

    fs::write(&asm, assembly).unwrap();
    fs::write(
        &harness,
        r#"#include <stdint.h>
#include <stdlib.h>
static uint64_t saved_environment;
static uint32_t saved_definition;
uint64_t wsm_closure_new(void *ctx, uint32_t definition, uint64_t environment) {
    (void)ctx;
    saved_definition = definition;
    saved_environment = environment;
    return UINT64_C(0x1001);
}
uint64_t wsm_closure_definition(void *ctx, uint64_t closure) {
    (void)ctx; (void)closure;
    return saved_definition;
}
uint64_t wsm_closure_environment(void *ctx, uint64_t closure) {
    (void)ctx; (void)closure;
    return saved_environment;
}
void wsm_fail(void *ctx, uint32_t code, uint64_t a, uint64_t b) {
    (void)ctx; (void)code; (void)a; (void)b;
    exit(77);
}
extern uint64_t wsm_entry(void *);
int main(void) {
    (void)wsm_entry(0);
    return 1;
}
"#,
    )
    .unwrap();

    let linked = Command::new("cc")
        .arg(&harness)
        .arg(&asm)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("cc must execute");
    assert!(
        linked.status.success(),
        "arity-mismatch witness must link: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("arity-mismatch witness must execute");

    let _ = fs::remove_file(asm);
    let _ = fs::remove_file(harness);
    let _ = fs::remove_file(executable);

    assert_eq!(
        run.status.code(),
        Some(77),
        "mismatched runtime closure arity must reach wsm_fail"
    );
}
