// CML-C-BACKEND-CONFORMANCE: c_backend.rs had only ever been run against
// a handful of hand-picked fixtures (tests/c_backend_test.rs), never the
// full tests/fixtures/conformance.my suite conformance_test.rs already
// exercises against compiler.rs -- this closes that gap the same way,
// for the same tier-1 "constitutive" fixtures (the seven primitives +
// quote/cond, the only forms both backends actually implement).
use std::fs;
use std::process::Command;

use cml::c_backend::CBackend;
use cml::lower;
use cml::macros::MacroExpander;
use cml::parser;

fn parse_conformance_line(line: &str) -> Option<(String, String)> {
    let expr_marker = "(expr . \"";
    let expr_start = line.find(expr_marker)? + expr_marker.len();
    let expected_marker_full = "\") (expected . \"";
    let expr_end = line[expr_start..].find(expected_marker_full)? + expr_start;
    let expr = &line[expr_start..expr_end];
    let expected_start = expr_end + expected_marker_full.len();
    let expected_end = line[expected_start..].find("\")")? + expected_start;
    let expected = &line[expected_start..expected_end];
    Some((expr.replace("\\\"", "\""), expected.replace("\\\"", "\"")))
}

#[test]
fn c_backend_matches_every_constitutive_tier1_fixture() {
    let fixture_path = "../my-lisp/tests/fixtures/conformance.my";
    let fixture_content = fs::read_to_string(fixture_path).expect("Failed to read conformance.my");

    let mut checked = 0;
    let mut failures = Vec::new();

    for (i, line) in fixture_content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || !line.contains("(tier . 1)") {
            continue;
        }
        // error-mode fixtures and anything c_backend/lower.rs doesn't
        // implement (defmacro, arithmetic beyond +, etc.) are out of
        // scope here -- same tier-1 "constitutive" subset both backends
        // actually cover.
        if line.contains("(error .") {
            continue;
        }
        // fpga-lisp/c_backend have no inexact-number tag; compiler_test/
        // conformance_test skip these too (compatibility.my's
        // tier-1-skip-reason).
        if line.contains("3.0") {
            continue;
        }
        let Some((expr_str, expected_str)) = parse_conformance_line(line) else {
            continue;
        };

        let Ok(exprs) = parser::parse(&expr_str) else {
            continue;
        };
        let exprs = MacroExpander::new().process(&exprs);
        let Ok(program) = lower::lower_program(&exprs) else {
            failures.push(format!("{expr_str}: lowering failed"));
            continue;
        };

        let mut backend = CBackend::new();
        let c_source = backend.compile_program(&program);

        let c_path = format!("c_backend_conf_{i}.c");
        let bin_path = format!("c_backend_conf_{i}");
        fs::write(&c_path, &c_source).unwrap();

        let compile = Command::new("gcc").arg(&c_path).arg("-o").arg(&bin_path).output().unwrap();
        if !compile.status.success() {
            failures.push(format!(
                "{expr_str}: gcc failed: {}",
                String::from_utf8_lossy(&compile.stderr)
            ));
            let _ = fs::remove_file(&c_path);
            continue;
        }

        let run = Command::new(format!("./{bin_path}")).output().unwrap();
        let actual = String::from_utf8_lossy(&run.stdout).trim().to_lowercase();
        let _ = fs::remove_file(&c_path);
        let _ = fs::remove_file(&bin_path);

        checked += 1;
        // cml's own front-end uppercases every identifier as its target-
        // symbol convention (originally for fpga-lisp's assembler, not a
        // real language rule) -- case-fold both sides so that convention
        // doesn't masquerade as a real mismatch here.
        let expected_lower = expected_str.to_lowercase();
        if actual != expected_lower {
            failures.push(format!("{expr_str}: expected {expected_str:?}, got {actual:?}"));
        }
    }

    assert!(checked >= 10, "expected to exercise the ten constitutive tier-1 fixtures, got {checked}");
    assert!(failures.is_empty(), "{} fixture(s) failed:\n{}", failures.len(), failures.join("\n"));
}
