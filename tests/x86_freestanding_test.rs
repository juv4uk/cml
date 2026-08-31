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
        CompileError::Unsupported("quoted string (target ABI has no string representation)")
    );
}

#[test]
fn closures_are_rejected_before_x86_emission() {
    let error = X86FreestandingBackend::new()
        .compile_program(&[Ir::Lambda {
            params: Params::Fixed(vec!["x".to_string()]),
            body: Box::new(Ir::Var("x".to_string())),
        }])
        .expect_err("general closures are not admitted by the x86 slice");
    assert_eq!(error, CompileError::Unsupported("lambda"));
}

#[test]
fn identity_lambda_application_is_admitted_by_beta_reduction() {
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
}

#[test]
fn unsupported_ir_and_bad_arity_fail_before_output_exists() {
    let backend = X86FreestandingBackend::new();
    assert_eq!(
        backend.compile_program(&[Ir::Var("X".to_string())]),
        Err(CompileError::Unsupported("unbound variable"))
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
        Err(CompileError::Unsupported("equal? primitive"))
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
