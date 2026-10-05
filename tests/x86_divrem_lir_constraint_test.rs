//! Native mechanism witnesses for cml#574.
//!
//! This slice is deliberately backend-local. It proves that destructive x86
//! DIV can flow through LIR, fixed-register allocation, and MachineInst
//! emission without adding register facts to shared semantic IR.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::machine_inst::{MachineInst, MachineItem, Provenance, X86Reg, assemble_program};
use cml::native_baseline::NativeExecutable;
use cml::x86_lir::{LirFunction, LirInst, LirTerminator};
use cml::x86_regalloc::{
    AllocLocation, allocate_registers_with_constraints, fixed_constraints_for_function,
};

#[derive(Clone, Copy)]
enum ResultKind {
    Quotient,
    Remainder,
}

fn divrem_function(numerator: u64, divisor: u64, result: ResultKind) -> LirFunction {
    assert!(divisor != 0);
    let provenance = Provenance::new(None, "x86-divrem-lir-mechanism");
    let mut function = LirFunction::new("divrem", provenance.clone());

    let numerator_value = function.alloc_vreg();
    let divisor_value = function.alloc_vreg();
    let low = function.alloc_vreg();
    let high = function.alloc_vreg();
    let result_value = function.alloc_vreg();

    let block = function
        .block_mut(function.entry)
        .expect("entry block must exist");

    block.instructions.push(LirInst::Const64 {
        dst: numerator_value,
        imm: numerator,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Const64 {
        dst: divisor_value,
        imm: divisor,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Copy {
        dst: low,
        src: numerator_value,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Const64 {
        dst: high,
        imm: 0,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::DivRem {
        low,
        high,
        divisor: divisor_value,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Copy {
        dst: result_value,
        src: match result {
            ResultKind::Quotient => low,
            ResultKind::Remainder => high,
        },
        provenance: provenance.clone(),
    });
    block.terminator = LirTerminator::Ret {
        val: Some(result_value),
        provenance,
    };

    function
}

#[test]
fn divrem_derives_short_lived_rax_rdx_constraints() {
    let function = divrem_function(20, 6, ResultKind::Remainder);
    let constraints = fixed_constraints_for_function(&function);

    assert_eq!(constraints.len(), 2);

    let plan = allocate_registers_with_constraints(&function, &constraints)
        .expect("bounded div-rem function must allocate");

    let div = function.blocks[0]
        .instructions
        .iter()
        .find_map(|inst| match inst {
            LirInst::DivRem { low, high, .. } => Some((*low, *high)),
            _ => None,
        })
        .expect("DivRem must exist");

    assert_eq!(
        plan.assignments.get(&div.0),
        Some(&AllocLocation::Reg(X86Reg::Rax))
    );
    assert_eq!(
        plan.assignments.get(&div.1),
        Some(&AllocLocation::Reg(X86Reg::Rdx))
    );
    assert!(function.dump().contains("divrem low="));
    assert!(plan.dump().contains("must=%rax"));
    assert!(plan.dump().contains("must=%rdx"));
}

#[test]
fn divrem_lir_emits_semantics_neutral_machine_div() {
    let function = divrem_function(20, 6, ResultKind::Remainder);
    let items = cml::x86_lir::lir_to_machine_items(&function).expect("emit div-rem LIR");

    let divisor = items.iter().find_map(|item| match item {
        MachineItem::Inst(MachineInst::DivReg {
            divisor,
            provenance,
        }) => {
            assert!(
                provenance.semantic_id.is_none(),
                "physical DIV must not mint a semantic identity"
            );
            Some(*divisor)
        }
        _ => None,
    });

    let divisor = divisor.expect("LIR div-rem must emit MachineInst::DivReg");
    assert_ne!(divisor, X86Reg::Rax);
    assert_ne!(divisor, X86Reg::Rdx);
}

#[test]
fn divrem_native_execution_returns_quotient_and_remainder() {
    for (kind, expected) in [(ResultKind::Quotient, 3_u64), (ResultKind::Remainder, 2_u64)] {
        let function = divrem_function(20, 6, kind);
        let items = cml::x86_lir::lir_to_machine_items(&function).expect("emit div-rem LIR");
        let bytes = assemble_program(&items).expect("assemble div-rem machine code");
        let result = NativeExecutable::load(&bytes).call();
        assert_eq!(result, expected);
    }
}

#[test]
fn optimizer_keeps_destructive_divrem_and_constrained_temps() {
    let mut function = divrem_function(20, 6, ResultKind::Remainder);

    let before = function.dump();
    assert!(before.contains("divrem low="));

    let _ = cml::x86_opt::optimize_lir(
        &mut function,
        cml::x86_opt::LocalOptConfig::all_enabled(),
    );

    let after = function.dump();
    assert!(
        after.contains("divrem low="),
        "DIV is destructive/may trap and must not be removed by DCE"
    );

    let constraints = fixed_constraints_for_function(&function);
    let plan = allocate_registers_with_constraints(&function, &constraints)
        .expect("optimized div-rem must remain allocatable");
    assert_eq!(plan.fixed_constraints.len(), 2);
}
