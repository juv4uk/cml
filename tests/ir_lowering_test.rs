// Proves ast::Expr -> ir::Ir lowering (docs/heterogeneous-backends.md step
// 1) covers every tier-1 conformance fixture the existing compiler.rs path
// already handles -- the acceptance bar for "the IR is well-defined", not
// yet "a second backend consumes it" (that's a later step in the doc).
use std::fs;

use cml::ir::Ir;
use cml::lower::lower_expr;
use cml::macros::MacroExpander;
use cml::parser;

fn parse_conformance_line(line: &str) -> Option<(String, String)> {
    let expr_marker = "(expr . \"";
    let expr_start = line.find(expr_marker)? + expr_marker.len();
    let expected_marker_full = "\") (expected . \"";
    let expr_end = line[expr_start..].find(expected_marker_full)? + expr_start;
    let expr = &line[expr_start..expr_end];
    Some((expr.replace("\\\"", "\""), String::new()))
}

fn parse_error_line(line: &str) -> bool {
    line.contains("(expr . \"") && line.contains("\") (error . \"")
}

#[test]
fn lowers_every_tier1_conformance_fixture() {
    let fixture_path = "../my-lisp/tests/fixtures/conformance.my";
    let fixture_content = fs::read_to_string(fixture_path).expect("Failed to read conformance.my");

    let mut checked = 0;
    let mut failures = Vec::new();

    for line in fixture_content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || !line.contains("(tier . 1)") {
            continue;
        }
        // Static-error fixtures never reach lowering in the real pipeline
        // (compiler.rs's own static_error check short-circuits first);
        // out of scope for this test, same as conformance_test.rs's split.
        if parse_error_line(line) {
            continue;
        }
        // fpga-lisp has no inexact-number tag; compiler_test/conformance_test
        // skip these too (compatibility.my's tier-1-skip-reason).
        if line.contains("3.0") {
            continue;
        }
        let Some((expr_str, _)) = parse_conformance_line(line) else {
            continue;
        };

        let exprs = match parser::parse(&expr_str) {
            Ok(e) => e,
            Err(e) => {
                failures.push(format!("{expr_str}: parse error {e:?}"));
                continue;
            }
        };
        let Ok(exprs) = MacroExpander::new().process(&exprs) else {
            failures.push(format!("{expr_str}: macro expansion failed"));
            continue;
        };
        checked += 1;
        for expr in &exprs {
            if let Err(e) = lower_expr(expr) {
                failures.push(format!("{expr_str}: {e}"));
            }
        }
    }

    assert!(checked > 20, "expected to actually exercise a meaningful number of fixtures, got {checked}");
    assert!(failures.is_empty(), "lowering failed for {} fixture(s):\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn lowers_the_real_length_pair_from_core_my() {
    let source = "(def length-onto (lambda (values acc) (cond ((atom values) acc) (t (length-onto (cdr values) (+ acc 1)))))) (def length (lambda (values) (length-onto values 0))) (length (quote (a b c)))";
    let exprs = parser::parse(source).unwrap();
    for expr in &exprs {
        let ir = lower_expr(expr).unwrap_or_else(|e| panic!("lowering failed: {e}"));
        // Sanity: every top-level form here is a def or an application,
        // never a bare literal falling through unexpectedly.
        assert!(matches!(ir, Ir::Def { .. } | Ir::App { .. }));
    }
}
