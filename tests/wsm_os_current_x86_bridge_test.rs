use cml::compiler_mechanism::RichCompilerMechanismRef;
use cml::ir::{Ir, PrimOp, Quoted};
use cml::x86_freestanding::{CompileError, X86FreestandingBackend};
use std::collections::BTreeSet;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn current_mechanism(mechanism: RichCompilerMechanismRef, args: Vec<Ir>) -> Ir {
    Ir::Prim {
        op: PrimOp::CompilerMechanism(mechanism),
        args,
    }
}

fn sym(name: &str) -> Ir {
    Ir::Quote(Quoted::Sym {
        uppercased: name.to_uppercase(),
        original: name.to_string(),
    })
}

fn assemble_and_undefined_symbols(assembly: &str) -> BTreeSet<String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-current-wsm-os-{}-{nonce}",
        std::process::id()
    ));
    let source = base.with_extension("s");
    let object = base.with_extension("o");
    fs::write(&source, assembly).expect("write generated assembly");

    let assembled = Command::new("cc")
        .args(["-c", "-x", "assembler"])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("assembler must execute");
    assert!(
        assembled.status.success(),
        "assembler failed: {}\n--- generated assembly ---\n{assembly}",
        String::from_utf8_lossy(&assembled.stderr)
    );

    let nm = Command::new("nm")
        .arg("-u")
        .arg(&object)
        .output()
        .expect("nm must execute");
    assert!(nm.status.success(), "nm failed");

    let _ = fs::remove_file(source);
    let _ = fs::remove_file(object);

    String::from_utf8(nm.stdout)
        .expect("nm output is UTF-8")
        .lines()
        .filter_map(|line| line.split_whitespace().last().map(str::to_string))
        .collect()
}

#[test]
fn verified_structural_current_mechanisms_reach_wsm_runtime_without_sid_adapter() {
    let list = Ir::Quote(Quoted::List(vec![Quoted::Sym {
        uppercased: "A".into(),
        original: "A".into(),
    }]));
    let program = vec![
        current_mechanism(
            RichCompilerMechanismRef::PairConstruct,
            vec![sym("head"), Ir::Nil],
        ),
        current_mechanism(RichCompilerMechanismRef::SelectorHead, vec![list.clone()]),
        current_mechanism(RichCompilerMechanismRef::SelectorTail, vec![list]),
    ];

    let backend = X86FreestandingBackend::new();
    let first = backend
        .compile_program(&program)
        .expect("verified structural current mechanisms must compile");
    let second = backend
        .compile_program(&program)
        .expect("second compile must succeed");

    assert_eq!(first, second, "freestanding assembly must be deterministic");
    assert!(first.contains(".globl wsm_entry"));
    assert!(first.contains("call wsm_cons"));
    assert!(first.contains("call wsm_car"));
    assert!(first.contains("call wsm_cdr"));

    let undefined = assemble_and_undefined_symbols(&first);
    assert_eq!(
        undefined,
        BTreeSet::from([
            "wsm_car".to_string(),
            "wsm_cdr".to_string(),
            "wsm_cons".to_string(),
        ])
    );
    let ratified: BTreeSet<String> = wsm_os_target::RUNTIME_IMPORTS
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    assert!(
        undefined.is_subset(&ratified),
        "current mechanism projection escaped the pinned target ABI: {undefined:?}"
    );
}

#[test]
fn exact_d1_current_mechanisms_fail_closed_until_target_carrier_is_ratified() {
    for mechanism in [
        RichCompilerMechanismRef::AtomPredicateD1,
        RichCompilerMechanismRef::AtomEqualityD1,
        RichCompilerMechanismRef::ConditionalD1,
    ] {
        let op = if mechanism == RichCompilerMechanismRef::ConditionalD1 {
            PrimOp::CompilerConditionalExactD1(mechanism)
        } else {
            PrimOp::CompilerMechanism(mechanism)
        };
        let args = match mechanism {
            RichCompilerMechanismRef::AtomEqualityD1 => vec![sym("a"), sym("a")],
            RichCompilerMechanismRef::ConditionalD1 => vec![sym("test"), sym("body")],
            _ => vec![sym("a")],
        };
        let error = X86FreestandingBackend::new()
            .compile_program(&[Ir::Prim { op, args }])
            .expect_err("current exact-D1 mechanism must not fall back to historical truth");
        assert_eq!(error, CompileError::UnsupportedCompilerMechanism(mechanism));
    }
}

#[test]
fn unverified_generic_prim_remains_rejected() {
    let error = X86FreestandingBackend::new()
        .compile_program(&[Ir::Prim {
            op: PrimOp::Cons,
            args: vec![sym("a"), Ir::Nil],
        }])
        .expect_err("generic Prim must not inherit current-SENS admission");
    assert_eq!(error, CompileError::UnsupportedVariant("Prim"));
}
