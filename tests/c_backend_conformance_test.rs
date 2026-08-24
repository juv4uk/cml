// CML-C-BACKEND-CONFORMANCE: c_backend.rs had only ever been run against
// a handful of hand-picked fixtures (tests/c_backend_test.rs), never the
// shared tests/fixtures/conformance.my suite. Every tier-1 fixture must now
// be accounted for as executed or as one explicit unsupported category;
// parser/admission failures are failures, never silent `continue`s.
use std::fs;
use std::process::Command;

use cml::c_backend::CBackend;
use cml::lower;
use cml::macros::MacroExpander;
use cml::parser;

const SUPPORTED_LANGUAGE_CONTRACT: (u32, u32) = (2, 0);

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

fn parse_contract_version(line: &str, field: &str) -> Option<(u32, u32)> {
    let marker = format!("({field} . (");
    let start = line.find(&marker)? + marker.len();
    let end = line[start..].find(')')? + start;
    let mut parts = line[start..end].split_whitespace();
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    (parts.next().is_none()).then_some((major, minor))
}

#[test]
fn parses_fixture_contract_gate() {
    let fixture = "((expr . \"x\") (since-contract . (2 1)))";
    assert_eq!(parse_contract_version(fixture, "since-contract"), Some((2, 1)));
}

#[test]
fn c_backend_matches_every_constitutive_tier1_fixture() {
    let fixture_path = "../my-lisp/tests/fixtures/conformance.my";
    let fixture_content = fs::read_to_string(fixture_path).expect("Failed to read conformance.my");

    let mut checked = 0;
    let mut selected = 0;
    let mut unsupported_errors = 0;
    let mut unsupported_inexact = 0;
    let mut unsupported_newer_contract = 0;
    let mut failures = Vec::new();

    for (i, line) in fixture_content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || !line.contains("(tier . 1)") {
            continue;
        }
        selected += 1;

        // These are explicit capability states, not silent skips. Contract
        // 2.1+ fixtures are upstream evidence but cannot be executed as proof
        // for CML's declared supported contract 2.0.
        match parse_contract_version(line, "since-contract") {
            Some(version) if version > SUPPORTED_LANGUAGE_CONTRACT => {
                unsupported_newer_contract += 1;
                continue;
            }
            Some(_) => {}
            None if line.contains("(since-contract") => {
                failures.push(format!("fixture line {}: malformed since-contract field", i + 1));
                continue;
            }
            None => {}
        }
        if line.contains("(error .") {
            unsupported_errors += 1;
            continue;
        }
        // fpga-lisp/c_backend have no inexact-number tag; compiler_test/
        // conformance_test skip these too (compatibility.my's
        // tier-1-skip-reason).
        if line.contains("3.0") {
            unsupported_inexact += 1;
            continue;
        }
        let Some((expr_str, expected_str)) = parse_conformance_line(line) else {
            failures.push(format!("fixture line {}: expected-value record was not admitted", i + 1));
            continue;
        };

        let exprs = match parser::parse(&expr_str) {
            Ok(exprs) => exprs,
            Err(error) => {
                failures.push(format!("{expr_str}: parser admission failed: {error:?}"));
                continue;
            }
        };
        let Ok(exprs) = MacroExpander::new().process(&exprs) else {
            failures.push(format!("{expr_str}: macro expansion failed"));
            continue;
        };
        let Ok(program) = lower::lower_program_with_first_class_builtins(&exprs) else {
            failures.push(format!("{expr_str}: lowering failed"));
            continue;
        };

        let mut backend = CBackend::new();
        let Ok(c_source) = backend.compile_program(&program) else {
            failures.push(format!("{expr_str}: C compilation failed"));
            continue;
        };

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

    assert!(failures.is_empty(), "{} fixture(s) failed:\n{}", failures.len(), failures.join("\n"));
    let accounted = checked
        + unsupported_errors
        + unsupported_inexact
        + unsupported_newer_contract;
    assert_eq!(
        accounted, selected,
        "every selected tier-1 fixture must be executed or assigned one explicit unsupported state"
    );
    assert!(
        unsupported_newer_contract > 0,
        "the current upstream suite should exercise the supported/upstream contract gate"
    );
    eprintln!(
        "tier-1 matrix: selected={selected} supported={checked} unsupported-error={unsupported_errors} \
         unsupported-inexact={unsupported_inexact} unsupported-newer-contract={unsupported_newer_contract}"
    );
}
