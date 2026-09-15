//! `cml-baseline`: CLI utility to generate reproducible code-quality and performance baseline reports.
//!
//! Usage:
//!   cargo run --bin cml-baseline
//!   cargo run --bin cml-baseline -- --out benchmarks/baseline-skylake.json
//!   cargo run --bin cml-baseline -- --sexpr

use cml::native_baseline::generate_baseline_report;
use std::env;
use std::fs;
use std::process::Command;

fn get_head_commit() -> String {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output();
    if let Ok(out) = output {
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    "unknown-head".to_string()
}

/// Reads the external/my-lisp submodule's actual checked-out commit —
/// never a hardcoded constant (SUBMODULE-DEPENDENCY-MODEL-2026-09-16).
fn get_mylisp_pin() -> String {
    let output = Command::new("git")
        .args(["-C", "external/my-lisp", "rev-parse", "HEAD"])
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    "unknown-mylisp-pin".to_string()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let commit = get_head_commit();
    let mylisp_pin = get_mylisp_pin();
    let report = generate_baseline_report(&commit, &mylisp_pin);

    let mut out_file = None;
    let mut use_sexpr = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--sexpr" => use_sexpr = true,
            "--json" => use_sexpr = false,
            "--out" => {
                if i + 1 < args.len() {
                    out_file = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "-h" | "--help" => {
                println!("Usage: cml-baseline [--json | --sexpr] [--out <file>]");
                return;
            }
            _ => {}
        }
        i += 1;
    }

    let rendered = if use_sexpr {
        report.to_sexpr()
    } else {
        report.to_json()
    };

    if let Some(path) = out_file {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&path, &rendered).expect("write baseline report to file");
        eprintln!("Baseline report saved to: {path}");
    } else {
        println!("{rendered}");
    }
}
