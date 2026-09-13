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

#[test]
fn direct_nullary_lambda_witness() {
    let expected = wsm_os_target::encode_fixnum(42).expect("42 is a target fixnum");
    let (ok, out, err) = run_witness("((lambda () 42))", expected, true);
    assert!(ok, "direct nullary lambda must execute; stdout={out} stderr={err}");
}

#[test]
fn direct_binary_lambda_witness() {
    let expected = wsm_os_target::encode_fixnum(42).expect("42 is a target fixnum");
    let (ok, out, err) = run_witness("((lambda (x y) (+ x y)) 12 30)", expected, true);
    assert!(ok, "direct binary lambda must execute; stdout={out} stderr={err}");
}

#[test]
fn direct_five_argument_lambda_witness() {
    let expected = wsm_os_target::encode_fixnum(15).expect("15 is a target fixnum");
    let (ok, out, err) = run_witness(
        "((lambda (a b c d e) (+ a (+ b (+ c (+ d e))))) 1 2 3 4 5)",
        expected,
        true,
    );
    assert!(ok, "direct 5-arg lambda must execute; stdout={out} stderr={err}");
}

#[test]
fn direct_lambda_with_lexical_capture_witness() {
    let expected = wsm_os_target::encode_fixnum(123).expect("123 is a target fixnum");
    let (ok, out, err) = run_witness(
        "(let ((x 100)) ((lambda (a b) (+ x (+ a b))) 20 3))",
        expected,
        true,
    );
    assert!(ok, "direct lambda with capture must execute; stdout={out} stderr={err}");
}

#[test]
fn direct_variadic_lambda_witness() {
    let expected = wsm_os_target::encode_fixnum(12).expect("12 is a target fixnum");
    let (ok, out, err) = run_witness(
        "((lambda (a b . rest) (+ (car rest) (+ (car (cdr rest)) (car (cdr (cdr rest)))))) 1 2 3 4 5)",
        expected,
        true,
    );
    assert!(ok, "direct variadic lambda must execute; stdout={out} stderr={err}");
}

#[test]
fn direct_all_rest_lambda_witness() {
    let expected = wsm_os_target::encode_fixnum(6).expect("6 is a target fixnum");
    let (ok, out, err) = run_witness(
        "((lambda args (+ (car args) (+ (car (cdr args)) (car (cdr (cdr args)))))) 1 2 3)",
        expected,
        true,
    );
    assert!(ok, "direct all-rest lambda must execute; stdout={out} stderr={err}");
}

#[test]
fn direct_variadic_empty_rest_witness() {
    let expected = wsm_os_target::encode_fixnum(42).expect("42 is a target fixnum");
    let (ok, out, err) = run_witness(
        "((lambda (a b . rest) (cond ((eq rest ()) (+ a b)) (t 0))) 20 22)",
        expected,
        true,
    );
    assert!(ok, "direct variadic lambda with empty rest must execute; stdout={out} stderr={err}");
}

fn run_list_witness(
    source: &str,
    expected_fixnums: &[i64],
) -> (bool, String, String) {
    let expressions = parser::parse(source).expect("fixture must parse");
    let program = lower::lower_program(&expressions).expect("fixture must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("fixture must compile");

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock must be after epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-x86-list-witness-{}-{nonce}",
        std::process::id()
    ));
    let asm_path = base.with_extension("s");
    let c_path = base.with_extension("c");
    let exe_path = base.with_extension("bin");

    fs::write(&asm_path, assembly).expect("write generated assembly");

    let nil = wsm_os_target::NIL;
    let mut checks = String::new();
    for (i, val) in expected_fixnums.iter().enumerate() {
        let enc = wsm_os_target::encode_fixnum(*val).unwrap();
        checks.push_str(&format!(
            "    if (curr == UINT64_C({nil})) return 10 + {i};\n    if (wsm_car(0, curr) != UINT64_C({enc})) return 20 + {i};\n    curr = wsm_cdr(0, curr);\n"
        ));
    }
    checks.push_str(&format!("    if (curr != UINT64_C({nil})) return 99;\n"));

    let c_code = format!(
        "#include <stdint.h>\nextern uint64_t wsm_entry(void *);\nextern uint64_t wsm_car(void *, uint64_t);\nextern uint64_t wsm_cdr(void *, uint64_t);\nint main(void) {{\n    uint64_t curr = wsm_entry(0);\n{checks}    return 0;\n}}\n"
    );
    fs::write(&c_path, c_code).expect("write C witness harness");

    let mut cmd = Command::new("cc");
    cmd.arg(&c_path).arg(&asm_path);
    if let Some(runtime) = std::option_env!("WSM_NUCLEUS_ASM") {
        cmd.arg(runtime);
    } else {
        cmd.arg("/home/agents/GitHub/wsm-my-lisp/asm/nucleus.s");
    }
    let linked = cmd.arg("-o").arg(&exe_path).output().expect("cc must be available");

    if !linked.status.success() {
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
        format!("code={:?} err={}", run.status.code(), String::from_utf8_lossy(&run.stderr)),
    )
}

#[test]
fn row12_corpus_fixture_exact_witness() {
    let (ok, out, err) = run_list_witness(
        "((lambda (a b . rest) rest) 1 2 3 4 5)",
        &[3, 4, 5],
    );
    assert!(
        ok,
        "row 12 exact fixture ((lambda (a b . rest) rest) 1 2 3 4 5) -> (3 4 5) must execute; stdout={out} stderr={err}"
    );
}

#[test]
fn row13_corpus_fixture_exact_witness() {
    let (ok, out, err) = run_list_witness(
        "((lambda args args) 1 2 3)",
        &[1, 2, 3],
    );
    assert!(
        ok,
        "row 13 exact fixture ((lambda args args) 1 2 3) -> (1 2 3) must execute; stdout={out} stderr={err}"
    );
}