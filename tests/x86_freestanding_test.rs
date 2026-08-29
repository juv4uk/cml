use std::collections::BTreeSet;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::ir::{Ir, PrimOp, Quoted};
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
fn unsupported_ir_and_bad_arity_fail_before_output_exists() {
    let backend = X86FreestandingBackend::new();
    assert_eq!(
        backend.compile_program(&[Ir::Var("X".to_string())]),
        Err(CompileError::Unsupported("variable"))
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
            op: PrimOp::Add,
            args: vec![Ir::Int(1), Ir::Int(2)],
        }]),
        Err(CompileError::Unsupported("add primitive"))
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
    let program = vec![
        Ir::Cond {
            branches: vec![
                (Ir::Nil, Ir::Int(1)),
                (Ir::True, Ir::Int(42)),
            ]
        }
    ];
    let assembly = X86FreestandingBackend::new().compile_program(&program).unwrap();
    assert!(assembly.contains("cmpq %rcx, %rax"));
    assert!(assembly.contains("je .Lcond_branch_"));
}
