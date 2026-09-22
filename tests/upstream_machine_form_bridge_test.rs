#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::lisp_encoder_bridge::items_to_lisp_machine_forms;
use cml::machine_inst::{AluOp, MachineInst, MachineItem, Provenance, X86Reg};
use my_lisp::{Session, eval_program, load_core_library};
use std::fs;
use std::path::PathBuf;

fn upstream_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp")
        .join(relative)
}

fn load_source(relative: &str, session: &mut Session) {
    let path = upstream_path(relative);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} must exist: {error}", path.display()));
    eval_program(&source, session).unwrap_or_else(|error| {
        panic!(
            "{} must load as ordinary my-lisp: {error:?}",
            path.display()
        )
    });
}

fn prov() -> Provenance {
    Provenance::new(None, "cml#74 upstream-machine-form parity")
}

#[test]
fn cml_add_machine_items_match_pinned_lisp_owned_structured_forms() {
    let mut session = Session::default();
    load_core_library(&mut session).expect("core must bootstrap");
    load_source("lib/machine/encoding/x86-64.lisp", &mut session);
    load_source("lib/machine/admission/x86-64.lisp", &mut session);
    load_source("lib/machine/lowering/semantic-x86-64.lisp", &mut session);

    let upstream = eval_program("(x86-lower-add-u64-forms 10 32)", &mut session)
        .expect("pinned upstream ADD lowering must execute")
        .value
        .to_string();

    let items = vec![
        MachineItem::Inst(MachineInst::MovImm64 {
            dst: X86Reg::Rax,
            imm: 10,
            provenance: prov(),
        }),
        MachineItem::Inst(MachineInst::MovImm64 {
            dst: X86Reg::Rcx,
            imm: 32,
            provenance: prov(),
        }),
        MachineItem::Inst(MachineInst::AluRegReg {
            op: AluOp::Add,
            dst: X86Reg::Rax,
            src: X86Reg::Rcx,
            provenance: prov(),
        }),
        MachineItem::Inst(MachineInst::Ret { provenance: prov() }),
    ];

    let cml_projection =
        items_to_lisp_machine_forms(&items).expect("CML must project only admitted forms");

    assert_eq!(
        cml_projection, upstream,
        "CML machine selection must agree with pinned Lisp-owned structured forms before byte encoding"
    );
}

#[test]
fn upstream_machine_form_projection_fails_closed_on_non_form_items() {
    let items = vec![
        MachineItem::Label("local".to_string()),
        MachineItem::Inst(MachineInst::Ret { provenance: prov() }),
    ];

    assert!(
        items_to_lisp_machine_forms(&items).is_err(),
        "labels/unresolved control items are outside the first #74 form bridge"
    );
}

#[test]
fn stale_or_unadmitted_machine_instruction_fails_closed_before_upstream_encoding() {
    let stale = vec![MachineItem::Inst(MachineInst::AluRegReg {
        op: AluOp::Sub,
        dst: X86Reg::Rax,
        src: X86Reg::Rcx,
        provenance: prov(),
    })];

    assert!(
        items_to_lisp_machine_forms(&stale).is_err(),
        "an instruction outside the pinned #74 form slice must not silently become upstream machine truth"
    );
}

#[test]
fn direct_byte_and_gnu_paths_are_recorded_as_differential_not_normative() {
    let contract = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("contracts/upstream-machine-form-bridge.lisp"),
    )
    .expect("#74 authority classification contract must exist");

    assert!(contract.contains("(cml-direct-byte differential-bootstrap)"));
    assert!(contract.contains("(gnu-as differential-oracle)"));
    assert!(contract.contains("(upstream-admission-encoder normative-machine-contract)"));
    assert!(contract.contains("(raw-byte-authority cml-forbidden)"));
}
