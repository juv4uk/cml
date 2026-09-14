use cml::elf64::Elf64Executable;
use cml::machine_inst::{AluOp, MachineInst, MachineItem, Provenance, X86Reg, assemble_program};
use cml::machine_substrate::{
    MachineSubstrateError, assemble_sexp_program, expand_macro_atoms, item_to_sexp,
    parse_machine_program, program_to_sexp,
};
use std::fs;
use std::process::Command;

#[test]
fn machine_forms_round_trip_without_language_semantic_identity() {
    let prov = Provenance::new(None, "lisp-authored assembler substrate");
    let original = vec![
        MachineItem::Label("entry".to_string()),
        MachineItem::Inst(MachineInst::MovImm64 {
            dst: X86Reg::Rax,
            imm: 42,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluRegReg {
            op: AluOp::Add,
            dst: X86Reg::Rax,
            src: X86Reg::Rbx,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Ret { provenance: prov }),
    ];

    for item in &original {
        let text = item_to_sexp(item);
        let parsed = parse_machine_program(&text).expect("machine form must parse");
        assert_eq!(parsed.len(), 1);
        match &parsed[0] {
            MachineItem::Label(_) => {}
            MachineItem::Inst(inst) => assert!(inst.provenance().semantic_id.is_none()),
            MachineItem::JmpLabel { provenance, .. }
            | MachineItem::JccLabel { provenance, .. }
            | MachineItem::CallLabel { provenance, .. } => {
                assert!(provenance.semantic_id.is_none())
            }
        }
        assert_eq!(parsed[0], *item);
    }

    let whole = program_to_sexp(&original);
    assert_eq!(parse_machine_program(&whole).unwrap(), original);
}

#[test]
fn lisp_macro_atoms_expand_to_structured_machine_items() {
    let definitions = fs::read_to_string("contracts/machine-macro-atoms.lisp")
        .expect("machine macro contract must exist");
    let source = format!(
        "{definitions}\n(mov-imm rax 10)\n(add-imm rax 300)\n(tag-fixnum rax)\n(untag-fixnum rax)\n(fast-ret)\n"
    );

    let items = expand_macro_atoms(&source).expect("Lisp machine macros must expand");
    assert_eq!(items.len(), 5);
    for item in &items {
        if let MachineItem::Inst(inst) = item {
            assert!(
                inst.provenance().semantic_id.is_none(),
                "machine macro expansion must never allocate a language semantic ID"
            );
        }
    }

    let prov = Provenance::new(None, "lisp-authored assembler substrate");
    let expected = vec![
        MachineItem::Inst(MachineInst::MovImm64 {
            dst: X86Reg::Rax,
            imm: 10,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm32 {
            op: AluOp::Add,
            dst: X86Reg::Rax,
            imm: 300,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::ShlImm {
            reg: X86Reg::Rax,
            imm: 3,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::SarImm {
            reg: X86Reg::Rax,
            imm: 3,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Ret { provenance: prov }),
    ];
    assert_eq!(items, expected);
    assert_eq!(assemble_program(&items).unwrap(), assemble_program(&expected).unwrap());
}

#[test]
fn lisp_authored_machine_program_executes_natively_without_runtime() {
    let source = r#"
        (x86 label _start)
        (x86 mov-imm64 rax 10)
        (x86 alu-imm32 add rax 32)
        (x86 mov-reg-reg rdi rax)
        (x86 mov-imm64 rax 60)
        (x86 syscall)
    "#;

    let raw = assemble_sexp_program(source).expect("Lisp-authored machine program must assemble");
    let elf = Elf64Executable::new(raw);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("cml-lisp-asm-{nonce}"));
    elf.write_executable(&path).expect("write executable");
    let output = Command::new(&path).output().expect("run native executable");
    let _ = fs::remove_file(path);
    assert_eq!(output.status.code(), Some(42));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn machine_substrate_rejects_language_forms_and_lossy_immediates() {
    for source in [
        "(+ 10 32)",
        "(add rax rbx)",
        "(x86)",
        "(x86 invalid-op rax)",
        "(x86 mov-reg-reg not-a-register rax)",
    ] {
        assert!(parse_machine_program(source).is_err(), "must reject {source}");
    }

    for source in [
        "(x86 shl-imm rax -1)",
        "(x86 shl-imm rax 256)",
        "(x86 alu-imm8 add rax 128)",
        "(x86 alu-imm8 add rax -129)",
        "(x86 alu-imm32 add rax 2147483648)",
        "(x86 alu-imm32 add rax -2147483649)",
        "(x86 lea rax rsp 2147483648)",
        "(x86 jmp-rel32 2147483648)",
    ] {
        assert!(
            matches!(
                parse_machine_program(source),
                Err(MachineSubstrateError::InvalidImmediate(_))
            ),
            "out-of-range immediate must fail closed: {source}"
        );
    }
}
