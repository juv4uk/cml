use std::collections::BTreeSet;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::ir::{Ir, Params, PrimOp, Quoted};
use cml::lower;
use cml::parser;
use cml::x86_freestanding::{CompileError, X86FreestandingBackend};

fn frozen_fixture() -> Vec<Ir> {
    let expressions = parser::parse(wsm_os_target::FIRST_FIXTURE_SOURCE).unwrap();
    lower::lower_program(&expressions).unwrap()
}

fn assemble_and_undefined_symbols(assembly: &str, stem: &str) -> BTreeSet<String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-{stem}-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let object = base.with_extension("o");
    fs::write(&source, assembly).unwrap();

    let assembled = Command::new("cc")
        .args(["-c", "-x", "assembler"])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    assert!(
        assembled.status.success(),
        "assembler failed: {}\n--- generated assembly ---\n{assembly}",
        String::from_utf8_lossy(&assembled.stderr)
    );
    let nm = Command::new("nm").arg("-u").arg(&object).output().unwrap();
    assert!(nm.status.success(), "nm failed");
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(object);
    let undefined: BTreeSet<String> = String::from_utf8(nm.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.split_whitespace().last().map(str::to_string))
        .collect();
    let ratified: BTreeSet<String> = wsm_os_target::RUNTIME_IMPORTS
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    assert!(
        undefined.is_subset(&ratified),
        "object imported symbols outside wsm-os target ABI: {undefined:?}"
    );
    undefined
}

#[test]
fn frozen_cons_fixture_is_deterministic_and_assembles() {
    let backend = X86FreestandingBackend::new();
    let first = backend.compile_program(&frozen_fixture()).unwrap();
    let second = backend.compile_program(&frozen_fixture()).unwrap();
    assert_eq!(first, second);
    assert!(first.contains(".globl wsm_entry"));
    assert!(first.contains("call wsm_cons"));
    assert_eq!(
        assemble_and_undefined_symbols(&first, "frozen-cons"),
        BTreeSet::from(["wsm_cons".to_string()])
    );
}

#[test]
fn primitive_slice_uses_only_ratified_runtime_imports() {
    let program = vec![
        Ir::Prim {
            op: PrimOp::Car,
            args: vec![Ir::Quote(Quoted::List(vec![Quoted::Int(1)]))],
        },
        Ir::Prim {
            op: PrimOp::Cdr,
            args: vec![Ir::Quote(Quoted::List(vec![Quoted::Int(1)]))],
        },
        Ir::Prim {
            op: PrimOp::Eq,
            args: vec![Ir::Int(1), Ir::Int(1)],
        },
        Ir::Prim {
            op: PrimOp::Atom,
            args: vec![Ir::Int(1)],
        },
    ];
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .unwrap();
    assert_eq!(
        assemble_and_undefined_symbols(&assembly, "primitives"),
        BTreeSet::from([
            "wsm_atom".to_string(),
            "wsm_car".to_string(),
            "wsm_cdr".to_string(),
            "wsm_cons".to_string(),
            "wsm_eq".to_string(),
        ])
    );
}

#[test]
fn symbols_are_image_local_and_ordered_independently_of_traversal() {
    let assembly = X86FreestandingBackend::new()
        .compile_program(&[
            Ir::Quote(Quoted::Sym("Z".to_string())),
            Ir::Quote(Quoted::Sym("A".to_string())),
        ])
        .unwrap();
    let a = wsm_os_target::encode_symbol(1).unwrap();
    let z = wsm_os_target::encode_symbol(2).unwrap();
    assert!(assembly.contains(&format!("movabsq ${a}, %rax")));
    assert!(assembly.contains(&format!("movabsq ${z}, %rax")));
}

#[test]
fn quoted_strings_are_rejected_instead_of_being_collapsed_into_symbols() {
    let error = X86FreestandingBackend::new()
        .compile_program(&[Ir::Quote(Quoted::Str("notes/today".to_string()))])
        .expect_err("target ABI has no string representation");
    assert_eq!(
        error,
        CompileError::UnsupportedVariant("quoted string (target ABI has no string representation)")
    );
}

#[test]
fn unary_closure_value_is_materialized_in_the_runtime_arena() {
    let assembly = X86FreestandingBackend::new()
        .compile_program(&[Ir::Lambda {
            params: Params::Fixed(vec!["x".to_string()]),
            body: Box::new(Ir::Var("x".to_string())),
        }])
        .expect("bounded unary closure should be materialized");
    assert!(assembly.contains("call wsm_closure_new"));
    assert!(assembly.contains(".Lclosure_1:"));
}

#[test]
fn escaping_captured_closure_source_reaches_x86_admission() {
    let expressions = parser::parse(
        "(((lambda (x) (lambda (y) (cons x (cons y (quote ()))))) (quote A)) (quote B))",
    )
    .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("escaping captured closure should reach the bounded target slice");
    assert!(assembly.contains("call wsm_closure_new"));
    assert!(assembly.contains("call wsm_closure_environment"));
    assert!(assembly.contains("call wsm_closure_definition"));
    assert!(assembly.contains("call .Lclosure_"));
}

#[test]
fn pci_config_calls_are_explicit_target_abi_imports() {
    let expressions =
        parser::parse("((lambda (pci) (pci-config-read16 pci 0 5 0 0)) (pci-config-capability))")
            .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("bounded PCI capability calls should reach the x86 target profile");
    assert!(assembly.contains("call wsm_pci_config_capability"));
    assert!(assembly.contains("call wsm_pci_config_read16"));
    assert_eq!(
        assemble_and_undefined_symbols(&assembly, "pci-config-capability"),
        BTreeSet::from([
            "wsm_pci_config_capability".to_string(),
            "wsm_pci_config_read16".to_string(),
        ])
    );
}

#[test]
fn pci_config_read_composes_with_a_bounded_self_tail_call_retry_loop() {
    // CML-X86-CAPABILITY-CALL-IN-BOUNDED-TAIL-LOOP: the concrete shape a
    // real "wait for device status register bit" driver protocol needs --
    // a capability read inside the tail loop's own Cond test (the ready
    // check), with a separate countdown branch for timeout. Must go through
    // lower_program_with_tail_calls, the same entry point
    // wsm-os/crates/m4-generator actually uses for real fixtures --
    // lower_program alone does not preserve the TailSelfCall shape.
    let expressions = parser::parse(
        "(def wait-ready
           (lambda (pci tries)
             (cond ((eq (pci-config-read16 pci 0 5 0 0) 1) (quote ok))
                   ((eq tries 0) (quote timeout))
                   (t (wait-ready pci (- tries 1))))))
         (wait-ready (pci-config-capability) 3)",
    )
    .unwrap();
    let program = lower::lower_program_with_tail_calls(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect(
            "a capability read inside a bounded tail-loop's own Cond test should compile -- \
             it already does, through the tail-call-aware lowering entry point; no prior test \
             exercised this combination",
        );
    assert!(assembly.contains("call wsm_pci_config_capability"));
    assert!(assembly.contains("call wsm_pci_config_read16"));
    assert!(
        assembly.contains("jmp .Ltcloop_"),
        "must be a real jmp loop, not a call chain"
    );
    assert_eq!(
        assemble_and_undefined_symbols(&assembly, "wait-ready-retry-loop"),
        BTreeSet::from([
            "wsm_pci_config_capability".to_string(),
            "wsm_pci_config_read16".to_string(),
            "wsm_eq".to_string(),
            // Checked subtraction's overflow path, same as
            // checked_add_and_sub_produce_inline_arithmetic below.
            "wsm_fail".to_string(),
        ])
    );
}

#[test]
fn pci_config_call_arity_is_fail_closed() {
    let expressions = parser::parse("(pci-config-read16 0 5 0 0)").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    assert_eq!(
        X86FreestandingBackend::new().compile_program(&program),
        Err(CompileError::InvalidArity {
            operation: "pci-config-read16",
            expected: 5,
            actual: 4,
        })
    );
}

#[test]
fn identity_lambda_application_emits_a_real_machine_call() {
    let program = vec![Ir::App {
        func: Box::new(Ir::Lambda {
            params: Params::Fixed(vec!["x".to_string()]),
            body: Box::new(Ir::Var("x".to_string())),
        }),
        args: vec![Ir::Int(7)],
    }];
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("identity lambda is the first admitted application");
    let encoded = wsm_os_target::encode_fixnum(7).unwrap();
    assert!(assembly.contains(&format!("movabsq ${encoded}, %rax")));
    assert!(assembly.contains("call .Llambda_"));
    assert!(assembly.contains("movq %rsi, 0(%rsp)"));
    assert!(assembly.contains("movq 0(%rsp), %rax"));
}

#[test]
fn identity_lambda_source_reaches_x86_admission() {
    let expressions = parser::parse("((lambda (x) x) 7)").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("source identity lambda should reach the admitted slice");
    let encoded = wsm_os_target::encode_fixnum(7).unwrap();
    assert!(assembly.contains(&format!("movabsq ${encoded}, %rax")));
    assert!(assembly.contains("call .Llambda_"));
}

#[test]
fn single_argument_lambda_body_uses_a_real_lexical_frame() {
    let expressions = parser::parse("((lambda (x) (cons x (quote ()))) (quote A))").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("one-argument lambda body should use the bounded lexical frame");
    assert!(assembly.contains("call .Llambda_"));
    assert!(assembly.contains("movq %rsi, 0(%rsp)"));
    assert!(assembly.contains("call wsm_cons"));
}

#[test]
fn nested_lambda_copies_a_captured_outer_binding() {
    let expressions = parser::parse(
        "((lambda (x) ((lambda (y) (cons x (cons y (quote ())))) (quote B))) (quote A))",
    )
    .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("nested lambda should closure-convert the bounded outer binding");
    assert_eq!(assembly.matches("call .Llambda_").count(), 2);
    assert!(assembly.contains("movq %rsp, %rdx"));
    assert!(assembly.contains("movq 0(%rdx), %rax"));
}

#[test]
fn unsupported_ir_and_bad_arity_fail_before_output_exists() {
    let backend = X86FreestandingBackend::new();
    assert_eq!(
        backend.compile_program(&[Ir::Var("X".to_string())]),
        Err(CompileError::UnsupportedVariant("Var (unbound)"))
    );
    assert_eq!(
        backend.compile_program(&[Ir::Prim {
            op: PrimOp::Car,
            args: vec![],
        }]),
        Err(CompileError::InvalidArity {
            operation: "car",
            expected: 1,
            actual: 0,
        })
    );
    assert_eq!(
        backend.compile_program(&[Ir::Prim {
            op: PrimOp::EqualP,
            args: vec![Ir::Int(1), Ir::Int(1)],
        }]),
        Err(CompileError::UnsupportedVariant("equal? primitive"))
    );
}

#[test]
fn checked_add_and_sub_produce_inline_arithmetic() {
    let backend = X86FreestandingBackend::new();

    // Simple add: 1 + 2 = 3 — assembly must not call any runtime function.
    let add_asm = backend
        .compile_program(&[Ir::Prim {
            op: PrimOp::Add,
            args: vec![Ir::Int(1), Ir::Int(2)],
        }])
        .unwrap();
    assert!(add_asm.contains("sarq $3,"), "add must decode fixnum");
    assert!(add_asm.contains("addq"), "add must use addq");
    assert!(add_asm.contains("wsm_fail"), "add must guard overflow path");
    assert!(!add_asm.contains("call wsm_add"), "no runtime wsm_add call");

    // Simple sub: 5 - 3 = 2 — assembly must not call any runtime function.
    let sub_asm = backend
        .compile_program(&[Ir::Prim {
            op: PrimOp::Sub,
            args: vec![Ir::Int(5), Ir::Int(3)],
        }])
        .unwrap();
    assert!(sub_asm.contains("subq"), "sub must use subq");
    assert!(sub_asm.contains("wsm_fail"), "sub must guard overflow path");
    assert!(!sub_asm.contains("call wsm_sub"), "no runtime wsm_sub call");

    // Boundary: FIXNUM_MAX must assemble OK, FIXNUM_MAX+1 must be rejected at preflight.
    assert!(
        backend
            .compile_program(&[Ir::Int(wsm_os_target::FIXNUM_MAX)])
            .is_ok()
    );
    assert_eq!(
        backend.compile_program(&[Ir::Int(wsm_os_target::FIXNUM_MAX + 1)]),
        Err(CompileError::FixnumOutOfRange(
            wsm_os_target::FIXNUM_MAX + 1
        ))
    );

    // Overflow: the assembly for FIXNUM_MAX + 1 would overflow — but that's a
    // *runtime* overflow, not a preflight error, since both inputs are in range.
    let overflow_asm = backend
        .compile_program(&[Ir::Prim {
            op: PrimOp::Add,
            args: vec![Ir::Int(wsm_os_target::FIXNUM_MAX), Ir::Int(1)],
        }])
        .unwrap();
    // Must assemble correctly — the overflow is caught at runtime by wsm_fail.
    let symbols = assemble_and_undefined_symbols(&overflow_asm, "add-overflow");
    assert!(
        symbols.contains("wsm_fail"),
        "overflow path imports wsm_fail"
    );
}

#[test]
fn fixnum_range_is_owned_by_the_target_contract() {
    let backend = X86FreestandingBackend::new();
    assert!(
        backend
            .compile_program(&[Ir::Int(wsm_os_target::FIXNUM_MIN)])
            .is_ok()
    );
    assert!(
        backend
            .compile_program(&[Ir::Int(wsm_os_target::FIXNUM_MAX)])
            .is_ok()
    );
    assert_eq!(
        backend.compile_program(&[Ir::Int(wsm_os_target::FIXNUM_MAX + 1)]),
        Err(CompileError::FixnumOutOfRange(
            wsm_os_target::FIXNUM_MAX + 1
        ))
    );
}

#[test]
fn cond_branching_evaluates_only_truthy_branch() {
    let program = vec![Ir::Cond {
        branches: vec![(Ir::Nil, Ir::Int(1)), (Ir::True, Ir::Int(42))],
    }];
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .unwrap();
    assert!(assembly.contains("cmpq %rcx, %rax"));
    assert!(assembly.contains("je .Lcond_branch_"));
}

#[test]
fn explicit_self_tail_call_loop_lowers_without_calls() {
    let source =
        "(def loop (lambda (n) (cond ((eq n 0) (quote done)) (t (loop (- n 1)))))) (loop 5)";
    let expressions = parser::parse(source).unwrap();
    let ir = lower::lower_program_with_tail_calls(&expressions).unwrap();
    let backend = X86FreestandingBackend::new();
    let assembly = backend.compile_program(&ir).unwrap();

    // Assembles without unrecognized symbols.
    let _ = assemble_and_undefined_symbols(&assembly, "tail_call_loop");

    // Assert loop structure: jump to loop rather than call to self.
    assert!(assembly.contains("jmp .Ltcloop_"));
    // It shouldn't contain a recursive call to the function.
    // The only runtime calls should be for 'eq', 'wsm_fail' (for arithmetic overflow).
    let calls: Vec<&str> = assembly.lines().filter(|l| l.contains("call ")).collect();
    // Only wsm_eq and wsm_fail are expected. Add/Sub are inline. loop is inline (jmp).
    assert!(
        calls
            .iter()
            .all(|l| l.contains("wsm_eq") || l.contains("wsm_fail"))
    );
}

#[test]
fn literal_true_emits_the_canonical_symbol_word_not_the_manufactured_tag_true_immediate() {
    // Regression for WSM-OS-TARGET-TAG-TRUE-MANUFACTURED-PRIMITIVE
    // (ecosystem/plans/tasks.my, 2026-09-04): before this fix, `Ir::True`
    // compiled to `emit_immediate(wsm_os_target::TRUE)` -- the raw
    // manufactured Tag::True immediate wsm-os-runtime's own eq/atom
    // stopped producing back on 2026-09-02 (see wsm-os-runtime::CANONICAL_T's
    // doc comment). That left a real inconsistency reachable from compiled
    // WSM programs: a literal `t` in source and a runtime-computed `t`
    // (via (atom ...) or (eq ...)) would carry different Word bit patterns
    // -- not `eq` to each other despite both meaning canonical true.
    //
    // wsm_os_target::TRUE and the canonical symbol encoding are gnu-as
    // constant expressions, not runtime values, so this checks the emitted
    // immediate directly rather than requiring execution: the assembly
    // must contain the SAME word wsm-os-runtime::CANONICAL_T computes
    // (encode_symbol(SYMBOL_ID_MAX)), and must NOT contain the old raw
    // TRUE immediate.
    let canonical_t = wsm_os_target::encode_symbol(wsm_os_target::SYMBOL_ID_MAX)
        .expect("SYMBOL_ID_MAX must encode as a valid symbol word");
    assert_ne!(
        canonical_t,
        wsm_os_target::TRUE,
        "test assumption broken: canonical t and the old manufactured TRUE \
         immediate must differ for this regression test to mean anything"
    );

    let program = vec![Ir::True];
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .unwrap();
    let _ = assemble_and_undefined_symbols(&assembly, "literal_true");

    assert!(
        assembly.contains(&format!("movabsq ${canonical_t}, %rax")),
        "expected the canonical symbol word {canonical_t} to be emitted for literal t:\n{assembly}"
    );
    assert!(
        !assembly.contains(&format!("movabsq ${}, %rax", wsm_os_target::TRUE)),
        "the old manufactured TAG_TRUE immediate {} must no longer be emitted:\n{assembly}",
        wsm_os_target::TRUE
    );
}

// Note: The actual run test for CML-X86-DEF-BOUNDED-SELF-TAIL-RECURSIVE-FUNCTION
// is implemented in wsm-my-lisp/harness/src/countdown-100k.rs which uses
// global_asm! to include CML-generated assembly and calls wsm_entry from Rust.
// That harness proves: tail-call jump instead of recursive call, constant stack
// frame across 100,000 iterations, and result matches my-lisp oracle ("done").
//
// This test verifies the CML compilation path produces the correct assembly
// structure for a self-tail-recursive Def.
#[test]
fn self_tail_recursive_def_compiles_with_correct_structure() {
    // CML-X86-DEF-BOUNDED-SELF-TAIL-RECURSIVE-FUNCTION: admit a single
    // self-tail-recursive named function with no free variables.
    let expressions = parser::parse(
        "(def countdown (lambda (n)
              (cond ((eq n 0) (quote done))
                    (t (countdown (- n 1))))))
         (countdown 5)",
    )
    .unwrap();
    let program = lower::lower_program_with_tail_calls(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("self-tail-recursive def should compile");

    // Verify assembly structure matches the hand-written entry-countdown-5.s
    // which the wsm-my-lisp harness runs successfully (proving correct execution).
    assert!(
        assembly.contains(".Ltcloop_"),
        "must contain tail-call loop label"
    );
    assert!(
        assembly.contains("jmp .Ltcloop_"),
        "must contain tail-call jmp (not recursive call)"
    );
    assert!(
        assembly.contains("call wsm_eq"),
        "must call wsm_eq for equality check"
    );
    assert!(
        assembly.contains("call wsm_fail"),
        "must have overflow path to wsm_fail"
    );

    // Assemble to verify no syntax errors
    let _ = assemble_and_undefined_symbols(&assembly, "countdown-def");
}

#[test]
fn ordinary_named_def_runs_out_of_line_and_returns_its_value() {
    // First vertical slice of CML-CONSTITUTION-GENERAL-APPLICATION-DEFS:
    // the definition must not execute by fall-through from wsm_entry.
    let expressions = parser::parse(
        "(def increment (lambda (x) (+ x 1)))\n         (increment 41)",
    )
    .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("a fixed-arity ordinary named definition should compile");

    let entry_ret = assembly.find("wsm_entry:").and_then(|start| {
        assembly[start..]
            .find("    ret")
            .map(|offset| start + offset)
    });
    let function = assembly.find("\n.Ltcloop_");
    assert!(
        entry_ret.is_some_and(|entry_ret| function.is_some_and(|function| function > entry_ret)),
        "named function must be emitted after wsm_entry returns:\n{assembly}"
    );
    assert!(assembly.contains("call .Ltcloop_0"));

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-named-def-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let harness = base.with_extension("c");
    let executable = base.with_extension("bin");
    fs::write(&source, assembly).unwrap();
    fs::write(
        &harness,
        "#include <stdint.h>\n#include <stdlib.h>\nextern uint64_t wsm_entry(void *);\nvoid wsm_fail(void *ctx, unsigned code, uint64_t a, uint64_t b) { (void)ctx; (void)code; (void)a; (void)b; abort(); }\nint main(void) { return wsm_entry(0) == 339 ? 0 : 1; }\n",
    )
    .unwrap();
    let linked = Command::new("cc")
        .arg(&harness)
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "linking named-def runtime witness failed: {}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let run = Command::new(&executable).output().unwrap();
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(harness);
    let _ = fs::remove_file(executable);
    assert!(run.status.success(), "compiled (increment 41) must return fixnum 42");
}

#[test]
fn ordinary_self_recursion_uses_real_calls_and_returns_its_value() {
    // This is intentionally not a tail call: `down` must return before the
    // enclosing addition can finish, so every recursive step has a frame.
    let expressions = parser::parse(
        "(def down (lambda (n)\n             (cond ((eq n 0) 0)\n                   (t (+ 1 (down (- n 1)))))))\n         (down 4)",
    )
    .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("ordinary self recursion should compile");

    assert_eq!(
        assembly.matches("call .Ltcloop_0").count(),
        2,
        "one entry call plus one recursive call must be emitted:\n{assembly}"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-ordinary-recursion-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let harness = base.with_extension("c");
    let executable = base.with_extension("bin");
    fs::write(&source, assembly).unwrap();
    fs::write(
        &harness,
        "#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\nextern uint64_t wsm_entry(void *);\nuint64_t wsm_eq(void *ctx, uint64_t a, uint64_t b) { (void)ctx; return a == b ? 2 : 1; }\nvoid wsm_fail(void *ctx, unsigned code, uint64_t a, uint64_t b) { (void)ctx; (void)code; (void)a; (void)b; abort(); }\nint main(void) { uint64_t result = wsm_entry(0); printf(\"%llu\\n\", (unsigned long long)result); return result == 35 ? 0 : 1; }\n",
    )
    .unwrap();
    let linked = Command::new("cc")
        .arg(&harness)
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "linking ordinary-recursion runtime witness failed: {}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let run = Command::new(&executable).output().unwrap();
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(harness);
    let _ = fs::remove_file(executable);
    assert!(
        run.status.success(),
        "compiled (down 4) must return fixnum 4; got stdout={} stderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn forward_named_definition_is_admitted_before_its_source_definition() {
    let expressions = parser::parse(
        "(increment 41)\n         (def increment (lambda (x) (+ x 1)))",
    )
    .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("a forward fixed-arity named definition should compile");
    assert!(assembly.contains("call .Ltcloop_0"));
    assert!(assembly.contains("\n.Ltcloop_0:"));
    let _ = assemble_and_undefined_symbols(&assembly, "forward-named-def");
}

#[test]
fn mutual_recursion_runs_through_two_out_of_line_named_definitions() {
    let expressions = parser::parse(
        "(def even (lambda (n) (cond ((eq n 0) 1) (t (odd (- n 1))))))\n         (def odd (lambda (n) (cond ((eq n 0) 0) (t (even (- n 1))))))\n         (even 4)",
    )
    .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("mutual fixed-arity recursion should compile");
    assert!(assembly.contains("\n.Ltcloop_0:"));
    assert!(assembly.contains("\n.Ltcloop_1:"));
    assert!(assembly.contains("call .Ltcloop_0"));
    assert!(assembly.contains("call .Ltcloop_1"));

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-mutual-recursion-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let harness = base.with_extension("c");
    let executable = base.with_extension("bin");
    fs::write(&source, assembly).unwrap();
    fs::write(
        &harness,
        "#include <stdint.h>\n#include <stdlib.h>\nextern uint64_t wsm_entry(void *);\nuint64_t wsm_eq(void *ctx, uint64_t a, uint64_t b) { (void)ctx; return a == b ? 2 : 1; }\nvoid wsm_fail(void *ctx, unsigned code, uint64_t a, uint64_t b) { (void)ctx; (void)code; (void)a; (void)b; abort(); }\nint main(void) { return wsm_entry(0) == 11 ? 0 : 1; }\n",
    )
    .unwrap();
    let linked = Command::new("cc")
        .arg(&harness)
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "mutual-recursion witness must link");
    let run = Command::new(&executable).output().unwrap();
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(harness);
    let _ = fs::remove_file(executable);
    assert!(run.status.success(), "compiled (even 4) must return fixnum 1");
}

#[test]
fn two_argument_named_definition_uses_the_target_argument_registers() {
    let expressions = parser::parse(
        "(def add2 (lambda (a b) (+ a b)))\n         (add2 19 23)",
    )
    .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("a two-argument named definition should compile");
    assert!(assembly.contains("movq %rsi, 0(%rsp)"));
    assert!(assembly.contains("movq %rdx, 8(%rsp)"));

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-two-arg-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let harness = base.with_extension("c");
    let executable = base.with_extension("bin");
    fs::write(&source, assembly).unwrap();
    fs::write(
        &harness,
        "#include <stdint.h>\n#include <stdlib.h>\nextern uint64_t wsm_entry(void *);\nvoid wsm_fail(void *ctx, unsigned code, uint64_t a, uint64_t b) { (void)ctx; (void)code; (void)a; (void)b; abort(); }\nint main(void) { return wsm_entry(0) == 339 ? 0 : 1; }\n",
    )
    .unwrap();
    let linked = Command::new("cc")
        .arg(&harness)
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "two-argument witness must link");
    let run = Command::new(&executable).output().unwrap();
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(harness);
    let _ = fs::remove_file(executable);
    assert!(run.status.success(), "compiled (add2 19 23) must return fixnum 42");
}

#[test]
fn named_definition_uses_a_lexical_let_binding() {
    let expressions = parser::parse(
        "(def twice-plus-two (lambda (x)\n           (let ((once (+ x 1))) (+ once 1))))\n         (twice-plus-two 40)",
    )
    .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("let inside a named definition should compile");

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-let-def-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let harness = base.with_extension("c");
    let executable = base.with_extension("bin");
    fs::write(&source, assembly).unwrap();
    fs::write(
        &harness,
        "#include <stdint.h>\n#include <stdlib.h>\nextern uint64_t wsm_entry(void *);\nvoid wsm_fail(void *ctx, unsigned code, uint64_t a, uint64_t b) { (void)ctx; (void)code; (void)a; (void)b; abort(); }\nint main(void) { return wsm_entry(0) == 339 ? 0 : 1; }\n",
    )
    .unwrap();
    let linked = Command::new("cc").arg(&harness).arg(&source).arg("-o").arg(&executable).output().unwrap();
    assert!(linked.status.success(), "lexical-let witness must link");
    let run = Command::new(&executable).output().unwrap();
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(harness);
    let _ = fs::remove_file(executable);
    assert!(run.status.success(), "compiled lexical let program must return fixnum 42");
}
