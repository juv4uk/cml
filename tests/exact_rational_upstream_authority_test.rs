#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::process::Command;

use cml::c_backend::CBackend;
use cml::{lower, parser};

fn gcc_command() -> Command {
    let mut cmd = Command::new("gcc");
    if std::env::var("C_INCLUDE_PATH").is_err()
        && std::path::Path::new("/var/guix/profiles/shared/guix-profile/include").exists()
    {
        cmd.env(
            "C_INCLUDE_PATH",
            "/var/guix/profiles/shared/guix-profile/include",
        );
    }
    cmd
}

fn compile_and_run_first_class(code: &str, stem: &str) -> String {
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    let c_source = CBackend::new().compile_program(&program).unwrap();
    let c_path = format!("c_backend_{stem}_test.c");
    let bin_path = format!("c_backend_{stem}_test");
    fs::write(&c_path, &c_source).unwrap();

    let compile = gcc_command()
        .arg(&c_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);
    assert!(run.status.success(), "compiled C program failed");
    String::from_utf8(run.stdout).unwrap().trim().to_string()
}

#[test]
fn c_backend_exact_rational_result_is_owned_by_upstream_lisp_witness() {
    let (source, expected) = upstream_exact_rational_compiler_witness();
    assert_eq!(
        compile_and_run_first_class(&source, "upstream_exact_rational"),
        expected
    );
}
