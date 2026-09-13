use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use cml::{lower, parser, x86_freestanding::X86FreestandingBackend};

fn run_witness(
    source: &str,
    expected: u64,
    link_nucleus: bool,
) -> (bool, String, String) {
    let expressions = parser::parse(source).expect("fixture must parse");
    let program = lower::lower_program(&expressions).expect("fixture must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("top-level let fixture must compile");

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock must be after epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-x86-top-level-let-{}-{nonce}",
        std::process::id()
    ));
    let asm_path = base.with_extension("s");
    let c_path = base.with_extension("c");
    let exe_path = base.with_extension("bin");

    fs::write(&asm_path, assembly).expect("write generated assembly");
    fs::write(
        &c_path,
        format!(
            "#include <stdint.h>\nextern uint64_t wsm_entry(void *);\nint main(void) {{ return wsm_entry(0) == UINT64_C({expected}) ? 0 : 1; }}\n"
        ),
    )
    .expect("write C witness harness");

    let mut cmd = Command::new("cc");
    cmd.arg(&c_path).arg(&asm_path);
    if let Some(runtime) = std::option_env!("WSM_NUCLEUS_ASM") {
        cmd.arg(runtime);
    } else if link_nucleus {
        cmd.arg("/home/agents/GitHub/wsm-my-lisp/asm/nucleus.s");
    }
    let linked = cmd.arg("-o").arg(&exe_path).output().expect("cc must be available");

    let linked_ok = linked.status.success();
    if !linked_ok {
        return (false, String::new(), format!("link failed: {}", String::from_utf8_lossy(&linked.stderr)));
    }

    let run = Command::new(&exe_path)
        .output()
        .expect("compiled witness must execute");

    let _ = fs::remove_file(&asm_path);
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(&exe_path);

    (
        run.status.success(),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        String::from_utf8_lossy(&run.stderr).into_owned(),
    )
}

#[test]
fn top_level_let_binding_read_computes_inferred_value() {
    let expected = wsm_os_target::encode_fixnum(15).expect("15 is a target fixnum");
    let (ok, out, err) = run_witness("(let ((x 10)) (+ x 5))", expected, true);
    assert!(
        ok,
        "top-level let must evaluate its lexical body; stdout={out} stderr={err}"
    );
}

#[test]
fn top_level_let_closure_value_can_be_applied_within_its_body() {
    let expected = wsm_os_target::encode_fixnum(42).expect("42 is a target fixnum");
    let (ok, out, err) = run_witness(
        "(let ((inc (lambda (x) (+ x 1)))) (inc 41))",
        expected,
        true,
    );
    assert!(
        ok,
        "closure value introduced by top-level let must dispatch through the closure ABI; stdout={out} stderr={err}"
    );
}