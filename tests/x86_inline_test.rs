//! Conformance, lexical capture, recursion safety, and differential parity tests
//! for Callable Inlining across Lowered Boundaries (#58).
//!
//! Verifies:
//! 1. Tiny pure arithmetic helpers are inlined and subsequently optimized by #55/#57/#56.
//! 2. Nested lambdas with lexical capture maintain exact variable bindings without capture.
//! 3. Recursive callables are safely bounded and rejected by the recursion policy.
//! 4. Unknown/dynamic callables refuse inlining and preserve fail-closed semantics.
//! 5. Differential parity: `inline off == inline on == lisp_oracle`.
//! 6. Inlining decisions, costs, and reasons are inspectable.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::machine_inst::assemble_program;
use cml::native_baseline::NativeExecutable;
use cml::x86_inline::{InlineConfig, inline_program};
use cml::x86_lir::{lir_to_machine_items, lower_ir_to_lir};
use cml::x86_opt::{LocalOptConfig, optimize_lir};
use cml::{lower, parser};
use my_lisp::{Session, eval_program, load_core_library};

fn parse_and_lower_ir(source: &str) -> cml::ir::Ir {
    let exprs =
        parser::parse(source).unwrap_or_else(|e| panic!("parse error for `{source}`: {e:?}"));
    let mut lowered = lower::lower_program(&exprs)
        .unwrap_or_else(|e| panic!("lowering error for `{source}`: {e}"));
    assert_eq!(lowered.len(), 1);
    lowered.remove(0)
}

fn lisp_oracle(source: &str) -> u64 {
    let mut session = Session::default();
    let _ = load_core_library(&mut session);
    eval_program(source, &mut session)
        .unwrap_or_else(|e| panic!("oracle eval error for `{source}`: {e:?}"))
        .value
        .to_string()
        .parse::<u64>()
        .unwrap_or_else(|e| panic!("oracle returned non-u64 for `{source}`: {e}"))
}

#[test]
fn test_tiny_arithmetic_helper_inlined() {
    let source = "(let ((add10 (lambda (x) (+ x 10)))) (+ (add10 20) (add10 12)))";
    let ir = parse_and_lower_ir(source);

    let (inlined_ir, decisions) = inline_program(&ir, &InlineConfig::default_enabled());

    // Acceptance criterion 1: inlining decisions inspectable with cost and reason
    assert_eq!(decisions.len(), 2);
    for d in &decisions {
        assert!(d.callee.eq_ignore_ascii_case("add10"));
        assert!(d.inlined);
        assert!(d.cost <= 30);
        assert!(d.reason.contains("inlined"));
    }

    // Lower to LIR and optimize
    let mut func = lower_ir_to_lir(&inlined_ir).expect("lower inlined IR to LIR");
    let report = optimize_lir(&mut func, LocalOptConfig::all_enabled());
    assert!(report.consts_folded >= 2);

    // Native execution
    let items = lir_to_machine_items(&func).expect("emit machine items");
    let bytes = assemble_program(&items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    let result = exec.call();

    // Acceptance criterion 2: semantic parity judged by Lisp oracle
    let oracle_val = lisp_oracle(source);
    assert_eq!(result, 52);
    assert_eq!(result, oracle_val);
}

#[test]
fn test_nested_lambda_lexical_capture() {
    let source = "(let ((a 10)) (let ((add_a (lambda (b) (+ a b)))) (add_a 32)))";
    let ir = parse_and_lower_ir(source);

    let (inlined_ir, decisions) = inline_program(&ir, &InlineConfig::default_enabled());

    assert_eq!(decisions.len(), 1);
    assert!(decisions[0].inlined);
    assert!(decisions[0].callee.eq_ignore_ascii_case("add_a"));

    let mut func = lower_ir_to_lir(&inlined_ir).expect("lower to LIR");
    let report = optimize_lir(&mut func, LocalOptConfig::all_enabled());
    assert!(report.consts_folded >= 1);

    let items = lir_to_machine_items(&func).expect("emit machine items");
    let bytes = assemble_program(&items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    let result = exec.call();

    let oracle_val = lisp_oracle(source);
    assert_eq!(result, 42);
    assert_eq!(result, oracle_val);
}

#[test]
fn test_recursive_callable_bounded_and_rejected() {
    // Recursive function calling itself: inliner must refuse expansion
    let source = "(def sum_down (lambda (n) (cond ((eq n 0) 0) (t (+ n (sum_down (- n 1)))))))";
    let ir = parse_and_lower_ir(source);

    let def_name = match &ir {
        cml::ir::Ir::Def { name, .. } => name.clone(),
        _ => panic!("expected Def"),
    };

    // Create an expression applying sum_down to 5: (sum_down 5)
    let app_ir = cml::ir::Ir::Let {
        bindings: vec![(
            def_name.clone(),
            match &ir {
                cml::ir::Ir::Def { value, .. } => (**value).clone(),
                _ => panic!("expected Def"),
            },
        )],
        body: Box::new(cml::ir::Ir::App {
            func: Box::new(cml::ir::Ir::Var(def_name)),
            args: vec![cml::ir::Ir::Int(5)],
        }),
    };

    let (_inlined_ir, decisions) = inline_program(&app_ir, &InlineConfig::default_enabled());

    // Verify that recursive calls within sum_down were rejected by recursion policy
    let recursive_decision = decisions
        .iter()
        .find(|d| !d.inlined && d.reason.contains("recursive"));
    assert!(
        recursive_decision.is_some(),
        "inliner must refuse unbounded expansion of recursive callable: got decisions {decisions:?}"
    );
}

#[test]
fn test_unknown_or_dynamic_callable_rejected() {
    // Calling an unknown variable without known lambda definition
    let app_ir = cml::ir::Ir::App {
        func: Box::new(cml::ir::Ir::Var("dynamic_runtime_fn".to_string())),
        args: vec![cml::ir::Ir::Int(42)],
    };

    let (_inlined_ir, decisions) = inline_program(&app_ir, &InlineConfig::default_enabled());

    assert_eq!(decisions.len(), 1);
    assert!(!decisions[0].inlined);
    assert!(decisions[0].reason.contains("unknown"));
}

#[test]
fn test_inline_off_reference_mode() {
    let source = "(let ((add10 (lambda (x) (+ x 10)))) (+ (add10 20) (add10 12)))";
    let ir = parse_and_lower_ir(source);

    let (unmodified_ir, decisions) = inline_program(&ir, &InlineConfig::disabled());

    assert_eq!(unmodified_ir, ir);
    assert!(decisions.is_empty());
}

#[test]
fn test_full_pipeline_cross_pass_synergy() {
    // End-to-end synergy:
    // inlining (#58) -> LIR CFG (#54) -> local optimization (#57) -> regalloc (#56) -> machine bytes (#52)
    let source = "(let ((square (lambda (x) (+ x x)))) (+ (square 10) (square 11)))";
    let ir = parse_and_lower_ir(source);

    let (inlined, decisions) = inline_program(&ir, &InlineConfig::default_enabled());
    assert_eq!(decisions.len(), 2);
    assert!(decisions.iter().all(|d| d.inlined));

    let mut func = lower_ir_to_lir(&inlined).expect("lower to LIR");
    let report = optimize_lir(&mut func, LocalOptConfig::all_enabled());
    assert!(report.consts_folded >= 3);

    let items = lir_to_machine_items(&func).expect("emit machine items via regalloc");
    let bytes = assemble_program(&items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    let result = exec.call();

    let oracle_val = lisp_oracle(source);
    assert_eq!(result, 42); // (10 + 10) + (11 + 11) = 20 + 22 = 42
    assert_eq!(result, oracle_val);
}
