// Proves ir::Ir is a real second-backend boundary, not just fpga-lisp-
// shaped by accident (docs/heterogeneous-backends.md step 2): compiles a
// def+lambda+primitive program through c_backend.rs, compiles the
// resulting C with a real `gcc`, runs it, and checks the printed result
// against the my-lisp reference value for the same source
// (`((lambda (x) (+ x 1)) 41)` -> `42`, verified live against the
// my-lisp CLI/oracle).
use std::fs;
use std::process::Command;

use cml::c_backend::CBackend;
use cml::lower;
use cml::parser;

#[test]
fn compiles_add1_to_c_and_runs_it() {
    let code = "(def add1 (lambda (x) (+ x 1))) (add1 41)";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program(&exprs).unwrap();
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).unwrap();

    let c_path = "c_backend_add1_test.c";
    let bin_path = "c_backend_add1_test";
    fs::write(c_path, &c_source).unwrap();

    let compile = Command::new("gcc")
        .arg(c_path)
        .arg("-o")
        .arg(bin_path)
        .output()
        .expect("failed to run gcc -- is it in PATH? (see manifest.scm's gcc-toolchain)");
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDOUT: {}\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stdout),
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&run.stdout);

    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    assert_eq!(stdout.trim(), "42", "expected 42 (matches my-lisp oracle for the same source), got: {stdout}");
}

#[test]
fn compiles_self_recursive_def_to_c_and_runs_it() {
    // Same fixture used to root-cause the fpga-lisp backend's R4/ENV-
    // clobber bug (e73f93a) -- here to prove the C backend's independent
    // letrec-placeholder-plus-backpatch (compile_def in c_backend.rs)
    // gets self-recursion right too, not just fixed-arity application.
    let code = "(def count (lambda (n) (cond ((eq n 0) 99) (t (count (+ n -1)))))) (count 3)";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program(&exprs).unwrap();
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).unwrap();

    let c_path = "c_backend_count_test.c";
    let bin_path = "c_backend_count_test";
    fs::write(c_path, &c_source).unwrap();

    let compile = Command::new("gcc").arg(c_path).arg("-o").arg(bin_path).output().unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);

    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    assert_eq!(stdout.trim(), "99", "expected 99 (matches my-lisp oracle: (count 3) -> 99), got: {stdout}");
}

#[test]
fn compiles_let_to_c_and_runs_it() {
    // CML-C-BACKEND-LET: compile_expr's Ir::Let arm (derives let via an
    // immediately-applied lambda, same technique compiler.rs uses) has
    // never actually been run before this test.
    let code = "(let ((x 5) (y 3)) (+ x y))";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program(&exprs).unwrap();
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).unwrap();

    let c_path = "c_backend_let_test.c";
    let bin_path = "c_backend_let_test";
    fs::write(c_path, &c_source).unwrap();

    let compile = Command::new("gcc").arg(c_path).arg("-o").arg(bin_path).output().unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);

    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    assert_eq!(stdout.trim(), "8", "expected 8 (matches my-lisp oracle: (let ((x 5) (y 3)) (+ x y)) -> 8), got: {stdout}");
}

#[test]
fn compiles_variadic_and_dotted_lambda_params_to_c_and_runs_it() {
    // CML-C-BACKEND-VARIADIC: compile_lambda previously panicked on
    // Params::Variadic/AllRest.
    let code = "(cons (car ((lambda args args) 1 2 3)) (car ((lambda (a . rest) rest) 1 2 3)))";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program(&exprs).unwrap();
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).unwrap();

    let c_path = "c_backend_variadic_test.c";
    let bin_path = "c_backend_variadic_test";
    fs::write(c_path, &c_source).unwrap();

    let compile = Command::new("gcc").arg(c_path).arg("-o").arg(bin_path).output().unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);

    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    assert_eq!(
        stdout.trim(),
        "(1 . 2)",
        "expected (1 . 2) (matches my-lisp oracle: car of bare-symbol-params args -> 1, car of dotted-rest -> 2), got: {stdout}"
    );
}

#[test]
fn compiles_quoted_list_access_to_c_and_runs_it() {
    // CML-C-BACKEND-QUOTED-LISTS: compile_quoted previously panicked on
    // Quoted::List/DottedList. car/(car (cdr ...)) into a quoted list
    // exercises the fix without depending on print_value's raw dotted-pair
    // format matching my-lisp's own list printer.
    let code = "(cons (car (quote (1 2 3))) (car (cdr (quote (1 2 3)))))";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program(&exprs).unwrap();
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).unwrap();

    let c_path = "c_backend_quoted_list_test.c";
    let bin_path = "c_backend_quoted_list_test";
    fs::write(c_path, &c_source).unwrap();

    let compile = Command::new("gcc").arg(c_path).arg("-o").arg(bin_path).output().unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);

    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    // print_value renders a cons pair as "(car . cdr)"; car=1, cdr=2 here.
    assert_eq!(
        stdout.trim(),
        "(1 . 2)",
        "expected (1 . 2) (matches my-lisp oracle: car of '(1 2 3) -> 1, car of cdr -> 2), got: {stdout}"
    );
}

#[test]
fn nested_def_returns_graceful_error() {
    // CML-C-BACKEND-ERROR-HANDLING: a nested `def` must produce a
    // CompileError::NestedDef instead of panicking.
    let code = "(def x (def y 1))";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program(&exprs).unwrap();
    let mut backend = CBackend::new();
    let err = backend.compile_program(&program).unwrap_err();
    assert!(
        matches!(err, cml::c_backend::CompileError::NestedDef),
        "expected NestedDef error, got: {err}"
    );
}

// --- Macro expansion error regression tests (CML-C-BACKEND-ERROR-HANDLING) ---

#[test]
fn macro_unbound_symbol_returns_graceful_error() {
    use cml::macros::{MacroError, MacroExpander};
    // A bare symbol `bar` in the macro body (not in a list) that's unbound
    let code = "(defmacro foo (x) bar) (foo 1)";
    let exprs = parser::parse(code).unwrap();
    let err = MacroExpander::new().process(&exprs).unwrap_err();
    assert!(
        matches!(err, MacroError::UnboundSymbol(ref s) if s == "bar"),
        "expected UnboundSymbol(\"bar\"), got: {err}"
    );
}

#[test]
fn macro_expected_operator_returns_graceful_error() {
    use cml::macros::{MacroError, MacroExpander};
    // A macro body that's a non-symbol list head: ((1 2) x)
    let code = "(defmacro foo (x) ((1 2) x)) (foo 1)";
    let exprs = parser::parse(code).unwrap();
    let err = MacroExpander::new().process(&exprs).unwrap_err();
    assert!(
        matches!(err, MacroError::ExpectedOperator),
        "expected ExpectedOperator, got: {err}"
    );
}

#[test]
fn macro_unsupported_form_returns_graceful_error() {
    use cml::macros::{MacroError, MacroExpander};
    let code = "(defmacro foo (x) (unknown-form x)) (foo 1)";
    let exprs = parser::parse(code).unwrap();
    let err = MacroExpander::new().process(&exprs).unwrap_err();
    assert!(
        matches!(err, MacroError::UnsupportedForm(ref s) if s == "unknown-form"),
        "expected UnsupportedForm(\"unknown-form\"), got: {err}"
    );
}
