//! Compile and exercise the REAL CML root x86 LIR and regalloc with pinned
//! modern SENS, without shadow types, SID aliases or a fake frontend.
//! This is target MECHANISM testing only, not SENS language/native parity.
#![allow(dead_code)]
#[path = "../../../../src/slot_vm.rs"]
mod slot_vm;
#[path = "../../../../src/compiler_mechanism.rs"]
mod compiler_mechanism;
#[path = "../../../../src/ir.rs"]
mod ir;
#[path = "../../../../src/machine_inst.rs"]
mod machine_inst;
#[path = "../../../../src/x86_lir.rs"]
mod x86_lir;
#[path = "../../../../src/x86_regalloc.rs"]
mod x86_regalloc;

use ir::{Ir, PrimOp};

fn require_machine_bytes(ir: &Ir, expected_alu: &str) {
    let cfg = x86_lir::lower_ir_to_lir(ir)
        .unwrap_or_else(|e| panic!("exact mechanism must lower: {e}"));
    let dump = cfg.dump();
    assert!(dump.contains(expected_alu), "LIR ALU not selected: {dump}");
    let items = x86_lir::lir_to_machine_items(&cfg)
        .unwrap_or_else(|e| panic!("actual x86 register allocation failed: {e}"));
    let binary = machine_inst::assemble_program(&items)
        .unwrap_or_else(|e| panic!("actual physical machine assembly failed: {e:?}"));
    assert!(!binary.is_empty(), "empty assembled machine code");
    println!("CML_ROOT_X86_MECHANISM_PASS op={expected_alu} bytes={}", binary.len());
}

fn main() {
    for (op, instruction) in [(PrimOp::Add, "add"), (PrimOp::Sub, "sub")] {
        let ir = Ir::Prim {
            op,
            args: vec![Ir::Int(10), Ir::Int(3)],
        };
        require_machine_bytes(&ir, instruction);
    }

    let condition = Ir::Cond {
        branches: vec![
            (
                Ir::Prim {
                    op: PrimOp::Eq,
                    args: vec![Ir::Int(5), Ir::Int(5)],
                },
                Ir::Int(42),
            ),
            (Ir::True, Ir::Int(99)),
        ],
    };
    let cfg = x86_lir::lower_ir_to_lir(&condition)
        .expect("CML machine comparison must form valid branch CFG");
    let dump = cfg.dump();
    assert!(dump.contains("branch_eq"), "missing machine branch: {dump}");

    // Names are not callable identities. CML may only accept machine
    // mechanisms after a verified SENS-domain compiler admission.
    for unsupported in [
        Ir::App {
            func: Box::new(Ir::Builtin("ADD".to_owned())),
            args: vec![Ir::Int(10), Ir::Int(3)],
        },
        Ir::Prim {
            op: PrimOp::Add,
            args: vec![Ir::Int(10)],
        },
    ] {
        assert!(x86_lir::lower_ir_to_lir(&unsupported).is_err(),
            "non-admitted machine form reached native LIR: {unsupported:?}");
    }
    println!("CML_ROOT_X86_MECHANISM_NEGATIVE_PASS surface_not_callable=true arity_checked=true");
}
