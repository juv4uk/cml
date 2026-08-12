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
    let c_source = backend.compile_program(&program);

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
    let c_source = backend.compile_program(&program);

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
