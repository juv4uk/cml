use cml::{lower, parser, x86_freestanding::X86FreestandingBackend};
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

/// Red-first Stage2 pressure test for wsm-my-lisp.
///
/// Lisp `let` evaluates all binding value forms in the enclosing environment,
/// then exposes the new bindings to the body. The x86 tail-body emitter must
/// therefore not let an earlier binding shadow a later binding's value form.
///
/// Correct semantics:
///   x=10, y=3
///   (let ((x y) (y x)) (- x y))
///     => (let ((x 3) (y 10)) ...) => -7
///
/// A sequential emitter that installs `x=3` before compiling the value of the
/// second binding misreads that second `x` as the new binding and returns 0.
#[test]
fn named_definition_let_bindings_are_parallel_not_sequential() {
    let source = r#"
        (def let-parallel
          (lambda (x y)
            (let ((x y)
                  (y x))
              (- x y))))
        (let-parallel 10 3)
    "#;

    let expressions = parser::parse(source).expect("Stage2 let fixture must parse");
    let program = lower::lower_program(&expressions).expect("Stage2 let fixture must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("lexical let inside a named definition must compile");

    let expected = wsm_os_target::encode_fixnum(-7).expect("-7 is a target fixnum");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock must be after epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-x86-let-parallel-{}-{nonce}",
        std::process::id()
    ));
    let asm_path = base.with_extension("s");
    let c_path = base.with_extension("c");
    let exe_path = base.with_extension("bin");

    fs::write(&asm_path, assembly).expect("write generated assembly");
    fs::write(
        &c_path,
        format!(
            "#include <stdint.h>\n#include <stdlib.h>\n\nextern uint64_t wsm_entry(void *);\nvoid wsm_fail(void *ctx, unsigned code, uint64_t a, uint64_t b) {{\n    (void)ctx; (void)code; (void)a; (void)b; abort();\n}}\n\nint main(void) {{\n    return wsm_entry(0) == UINT64_C({expected}) ? 0 : 1;\n}}\n"
        ),
    )
    .expect("write C witness harness");

    let linked = Command::new("cc")
        .arg(&c_path)
        .arg(&asm_path)
        .arg("-o")
        .arg(&exe_path)
        .output()
        .expect("cc must be available for x86 witness");
    assert!(
        linked.status.success(),
        "Stage2 let witness must link: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = Command::new(&exe_path)
        .output()
        .expect("compiled Stage2 let witness must execute");

    let _ = fs::remove_file(&asm_path);
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(&exe_path);

    assert!(
        run.status.success(),
        "x86 let bindings were not evaluated in the same enclosing environment; expected -7"
    );
}
