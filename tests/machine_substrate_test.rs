use cml::elf64::Elf64Executable;
use cml::machine_inst::{
    AluOp, CondCode, MachineInst, MachineItem, Provenance, X86Reg, assemble_program,
};
use cml::machine_substrate::{
    assemble_sexp_program, expand_macro_atoms, inst_to_sexp, item_to_sexp, parse_machine_program,
    program_to_sexp,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn run_gnu_as_assemble(gnu_text: &str) -> Vec<u8> {
    let tmp_dir = std::env::temp_dir();
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let s_path = tmp_dir.join(format!("substrate_gnu_oracle_{unique_id}.s"));
    let o_path = tmp_dir.join(format!("substrate_gnu_oracle_{unique_id}.o"));
    let bin_path = tmp_dir.join(format!("substrate_gnu_oracle_{unique_id}.bin"));

    fs::write(&s_path, gnu_text).expect("failed to write test assembly");

    let as_output = Command::new("as")
        .args([
            "--64",
            "-o",
            o_path.to_str().unwrap(),
            s_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run gnu as");
    assert!(
        as_output.status.success(),
        "as error: {}",
        String::from_utf8_lossy(&as_output.stderr)
    );

    let objcopy_output = Command::new("objcopy")
        .args([
            "-O",
            "binary",
            "--only-section=.text",
            o_path.to_str().unwrap(),
            bin_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run objcopy");
    assert!(
        objcopy_output.status.success(),
        "objcopy error: {}",
        String::from_utf8_lossy(&objcopy_output.stderr)
    );

    let bytes = fs::read(&bin_path).expect("failed to read binary");
    let _ = fs::remove_file(s_path);
    let _ = fs::remove_file(o_path);
    let _ = fs::remove_file(bin_path);
    bytes
}

#[test]
fn test_round_trip_machine_items() {
    let prov = Provenance::new(None, "lisp-authored assembler substrate");

    let sample_items = vec![
        MachineItem::Label("entry_point".to_string()),
        MachineItem::Inst(MachineInst::MovImm64 {
            dst: X86Reg::Rax,
            imm: 42,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::MovRegReg {
            dst: X86Reg::Rbx,
            src: X86Reg::Rax,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm32 {
            op: AluOp::Add,
            dst: X86Reg::Rax,
            imm: 100,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluRegReg {
            op: AluOp::Sub,
            dst: X86Reg::Rax,
            src: X86Reg::Rbx,
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
        MachineItem::Inst(MachineInst::ShrImm {
            reg: X86Reg::R11,
            imm: 1,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::PushReg {
            reg: X86Reg::Rbp,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::PopReg {
            reg: X86Reg::Rbp,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Lea {
            dst: X86Reg::R12,
            base: X86Reg::Rbp,
            disp: -16,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::MovStore {
            base: X86Reg::Rsp,
            disp: 8,
            src: X86Reg::Rdi,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::MovLoad {
            dst: X86Reg::Rsi,
            base: X86Reg::Rsp,
            disp: 8,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::TestRegReg {
            reg1: X86Reg::Rax,
            reg2: X86Reg::Rax,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Syscall {
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Rdtsc {
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Nop {
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Ret {
            provenance: prov.clone(),
        }),
        MachineItem::JmpLabel {
            target: "entry_point".to_string(),
            provenance: prov.clone(),
        },
        MachineItem::JccLabel {
            cond: CondCode::Equal,
            target: "entry_point".to_string(),
            provenance: prov.clone(),
        },
        MachineItem::CallLabel {
            target: "entry_point".to_string(),
            provenance: prov.clone(),
        },
    ];

    for original in &sample_items {
        let sexp = item_to_sexp(original);
        let parsed_items = parse_machine_program(&sexp).expect("failed to parse sexp");
        assert_eq!(parsed_items.len(), 1);
        let parsed = &parsed_items[0];

        // Strict boundary check: parsed item must not own a semantic ID
        match parsed {
            MachineItem::Label(_) => {}
            MachineItem::Inst(inst) => {
                assert!(
                    inst.provenance().semantic_id.is_none(),
                    "machine instruction must never have a language semantic ID"
                );
            }
            MachineItem::JmpLabel { provenance, .. }
            | MachineItem::JccLabel { provenance, .. }
            | MachineItem::CallLabel { provenance, .. } => {
                assert!(
                    provenance.semantic_id.is_none(),
                    "control item must never have a language semantic ID"
                );
            }
        }

        assert_eq!(original, parsed, "round-trip mismatch for sexp: {sexp}");
    }

    let program_sexp = program_to_sexp(&sample_items);
    let parsed_program =
        parse_machine_program(&program_sexp).expect("failed to parse full program");
    assert_eq!(sample_items, parsed_program);

    let single_inst = MachineInst::Rdtsc { provenance: prov };
    let inst_str = inst_to_sexp(&single_inst);
    assert_eq!(inst_str, "(x86 rdtsc)");
}

#[test]
fn test_sexp_machine_program_assembles_and_matches_direct_bytes() {
    let sexp_source = r#"
        (x86 label start)
        (x86 mov-imm64 rax 100)
        (x86 mov-imm64 rbx 42)
        (x86 alu-reg-reg add rax rbx)
        (x86 shl-imm rax 3)
        (x86 sar-imm rax 3)
        (x86 ret)
    "#;

    let items = parse_machine_program(sexp_source).expect("parse failed");
    let direct_bytes = assemble_program(&items).expect("assembly failed");

    // Also assemble via convenient helper
    let helper_bytes = assemble_sexp_program(sexp_source).expect("assemble_sexp_program failed");
    assert_eq!(direct_bytes, helper_bytes);

    // Verify against GNU as oracle
    let gnu_text = r#"
        .text
        .globl _start
        start:
            movabsq $100, %rax
            movabsq $42, %rbx
            addq %rbx, %rax
            shlq $3, %rax
            sarq $3, %rax
            ret
    "#;
    let oracle_bytes = run_gnu_as_assemble(gnu_text);
    assert_eq!(
        direct_bytes, oracle_bytes,
        "assembled bytes from sexp must match GNU as oracle byte-for-byte"
    );
}

#[test]
fn test_lisp_macro_atom_expansion_and_oracle_fidelity() {
    let macro_defs = fs::read_to_string("contracts/machine-macro-atoms.lisp")
        .expect("failed to read machine-macro-atoms.lisp");

    let invocation = r#"
        (mov-imm rax 10)
        (add-imm rax 300)
        (tag-fixnum rax)
        (untag-fixnum rax)
        (fast-ret)
    "#;

    let full_source = format!("{macro_defs}\n{invocation}");
    let expanded_items = expand_macro_atoms(&full_source).expect("macro expansion failed");

    assert_eq!(expanded_items.len(), 5);

    // Expected Rust constructors
    let prov = Provenance::new(None, "lisp-authored assembler substrate");
    let expected_items = vec![
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
        MachineItem::Inst(MachineInst::Ret {
            provenance: prov.clone(),
        }),
    ];

    assert_eq!(
        expanded_items, expected_items,
        "Lisp macro-atom expansion must produce exact structured MachineItems"
    );

    let assembled_bytes = assemble_program(&expanded_items).expect("assembly failed");
    let expected_bytes = assemble_program(&expected_items).expect("assembly failed");
    assert_eq!(assembled_bytes, expected_bytes);

    // Differential GNU as verification
    let gnu_text = r#"
        .text
        movabsq $10, %rax
        addq $300, %rax
        shlq $3, %rax
        sarq $3, %rax
        ret
    "#;
    let oracle_bytes = run_gnu_as_assemble(gnu_text);
    assert_eq!(assembled_bytes, oracle_bytes);
}

#[test]
fn test_lisp_authored_machine_program_native_execution() {
    // A complete standalone Linux ELF executable authored entirely as S-expression machine instructions:
    // Computes (+ 10 32) = 42, moves result to %rdi, and invokes sys_exit (60).
    let program_sexp = r#"
        (x86 label _start)
        (x86 mov-imm64 rax 10)
        (x86 alu-imm32 add rax 32)
        (x86 mov-reg-reg rdi rax)
        (x86 mov-imm64 rax 60)
        (x86 syscall)
    "#;

    let items = parse_machine_program(program_sexp).expect("failed to parse program");
    let raw_bytes = assemble_program(&items).expect("failed to assemble program");

    // Synthesize freestanding ELF64 directly without GNU ld
    let elf = Elf64Executable::new(raw_bytes);
    let elf_bytes = elf.to_bytes();

    let tmp_dir = std::env::temp_dir();
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let exe_path = tmp_dir.join(format!("cml_sexp_native_{unique_id}"));

    fs::write(&exe_path, &elf_bytes).expect("failed to write ELF file");
    let mut perms = fs::metadata(&exe_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&exe_path, perms).unwrap();

    let run_output = Command::new(&exe_path)
        .output()
        .expect("failed to execute synthesized ELF binary");

    let exit_code = run_output
        .status
        .code()
        .expect("process terminated by signal");
    let _ = fs::remove_file(&exe_path);

    assert_eq!(
        exit_code, 42,
        "Lisp-authored machine program must natively run and exit with code 42"
    );
}

#[test]
fn test_authority_boundary_rejection() {
    // Source language expressions or non-(x86 ...) forms must fail closed in machine substrate
    let invalid_forms = [
        "(+ 10 32)",
        "(1001 10 32)",
        "(add rax rbx)",
        "(x86)",
        "(x86 invalid-op rax)",
        "(x86 mov-reg-reg unknown_reg rbx)",
        "(x86 jcc-label bad_cond loop)",
    ];

    for form in &invalid_forms {
        let result = parse_machine_program(form);
        assert!(
            result.is_err(),
            "form '{form}' must be rejected by machine substrate parser"
        );
    }
}
