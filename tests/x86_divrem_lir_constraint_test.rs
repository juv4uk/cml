//! Native mechanism witnesses for cml#574.
//!
//! This slice is deliberately backend-local. It proves that destructive x86
//! DIV can flow through LIR, fixed-register allocation, and MachineInst
//! emission without adding register facts to shared semantic IR.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::machine_inst::{MachineInst, MachineItem, Provenance, X86Reg, assemble_program};
use cml::native_baseline::NativeExecutable;
use cml::x86_isel::{ScalarIselConfig, emit_machine_items_with_isel};
use cml::x86_lir::{LirAluOp, LirCond, LirFunction, LirInst, LirTerminator};
use cml::x86_regalloc::{
    AllocLocation, RegAllocPlan, SCRATCH_REG_A, allocate_registers_with_constraints,
    emit_machine_items_with_plan, fixed_constraints_for_function,
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


#[test]
fn optimizer_preserves_divrem_quotient_and_remainder_values() {
    for (kind, expected) in [(ResultKind::Quotient, 3_u64), (ResultKind::Remainder, 2_u64)] {
        let mut function = divrem_function(20, 6, kind);
        let _ = cml::x86_opt::optimize_lir(
            &mut function,
            cml::x86_opt::LocalOptConfig::all_enabled(),
        );

        let items = cml::x86_lir::lir_to_machine_items(&function)
            .expect("optimized div-rem must emit");
        let bytes = assemble_program(&items).expect("assemble optimized div-rem");
        let result = NativeExecutable::load(&bytes).call();
        assert_eq!(
            result, expected,
            "optimizer must not resurrect pre-DIV low/high facts"
        );
    }
}


#[test]
fn spilled_divisor_reloads_through_nonallocatable_scratch_before_div() {
    let function = divrem_function(20, 6, ResultKind::Remainder);
    let intervals = cml::x86_regalloc::build_live_intervals(&function);

    let (low, high, divisor) = function.blocks[0]
        .instructions
        .iter()
        .find_map(|inst| match inst {
            LirInst::DivRem {
                low,
                high,
                divisor,
                ..
            } => Some((*low, *high, *divisor)),
            _ => None,
        })
        .expect("DivRem must exist");

    let constraints = fixed_constraints_for_function(&function);
    let mut base = allocate_registers_with_constraints(&function, &constraints)
        .expect("baseline allocation");
    base.assignments.insert(low, AllocLocation::Reg(X86Reg::Rax));
    base.assignments.insert(high, AllocLocation::Reg(X86Reg::Rdx));
    base.assignments.insert(divisor, AllocLocation::SpillSlot(0));
    base.spill_count = base.spill_count.max(1);

    let plan = RegAllocPlan {
        assignments: base.assignments,
        spill_count: base.spill_count,
        intervals,
        fixed_constraints: base.fixed_constraints,
    };
    let items = emit_machine_items_with_plan(&function, &plan)
        .expect("spilled divisor must be reloadable for DIV");

    let div_index = items
        .iter()
        .position(|item| {
            matches!(
                item,
                MachineItem::Inst(MachineInst::DivReg {
                    divisor,
                    ..
                }) if *divisor == SCRATCH_REG_A
            )
        })
        .expect("DIV must consume the dedicated scratch register");

    assert!(
        items[..div_index].iter().any(|item| {
            matches!(
                item,
                MachineItem::Inst(MachineInst::MovLoad {
                    dst,
                    ..
                }) if *dst == SCRATCH_REG_A
            )
        }),
        "spilled divisor must be loaded into scratch before DIV"
    );
    assert_ne!(SCRATCH_REG_A, X86Reg::Rax);
    assert_ne!(SCRATCH_REG_A, X86Reg::Rdx);
}


#[test]
fn isel_does_not_reuse_pre_div_high_zero_as_post_div_remainder_constant() {
    let provenance = Provenance::new(None, "x86-divrem-isel-stale-const");
    let mut function = LirFunction::new("divrem_isel_stale_const", provenance.clone());

    let numerator = function.alloc_vreg();
    let divisor = function.alloc_vreg();
    let low = function.alloc_vreg();
    let high = function.alloc_vreg();
    let ten = function.alloc_vreg();
    let sum = function.alloc_vreg();

    let block = function.block_mut(function.entry).expect("entry block");
    block.instructions.push(LirInst::Const64 {
        dst: numerator,
        imm: 20,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Const64 {
        dst: divisor,
        imm: 6,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Copy {
        dst: low,
        src: numerator,
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
        divisor,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Const64 {
        dst: ten,
        imm: 10,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Alu {
        op: LirAluOp::Add,
        dst: sum,
        lhs: ten,
        rhs: high,
        provenance: provenance.clone(),
    });
    block.terminator = LirTerminator::Ret {
        val: Some(sum),
        provenance,
    };

    let constraints = fixed_constraints_for_function(&function);
    let plan = allocate_registers_with_constraints(&function, &constraints)
        .expect("div-rem + post-remainder ALU must allocate");
    let items = emit_machine_items_with_isel(
        &function,
        &plan,
        &ScalarIselConfig::default_skylake(),
    )
    .expect("isel must preserve post-DIV remainder value");

    let bytes = assemble_program(&items).expect("assemble isel div-rem witness");
    let result = NativeExecutable::load(&bytes).call();
    assert_eq!(
        result, 12,
        "20 % 6 is 2; post-DIV high must be 2, not stale pre-DIV constant 0"
    );
}


#[test]
fn branch_simplifier_does_not_treat_post_div_remainder_as_pre_div_zero() {
    let provenance = Provenance::new(None, "x86-divrem-branch-stale-const");
    let mut function = LirFunction::new("divrem_branch_stale_const", provenance.clone());

    let numerator = function.alloc_vreg();
    let divisor = function.alloc_vreg();
    let low = function.alloc_vreg();
    let high = function.alloc_vreg();
    let zero = function.alloc_vreg();
    let true_value = function.alloc_vreg();
    let false_value = function.alloc_vreg();

    let true_block = function.create_block();
    let false_block = function.create_block();
    let entry = function.entry;

    {
        let block = function.block_mut(entry).expect("entry block");
        block.instructions.push(LirInst::Const64 {
            dst: numerator,
            imm: 20,
            provenance: provenance.clone(),
        });
        block.instructions.push(LirInst::Const64 {
            dst: divisor,
            imm: 6,
            provenance: provenance.clone(),
        });
        block.instructions.push(LirInst::Copy {
            dst: low,
            src: numerator,
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
            divisor,
            provenance: provenance.clone(),
        });
        block.instructions.push(LirInst::Const64 {
            dst: zero,
            imm: 0,
            provenance: provenance.clone(),
        });
        block.instructions.push(LirInst::Cmp {
            lhs: high,
            rhs: zero,
            provenance: provenance.clone(),
        });
        block.terminator = LirTerminator::BranchCond {
            cond: LirCond::Equal,
            true_block,
            false_block,
            provenance: provenance.clone(),
        };
    }

    {
        let block = function.block_mut(true_block).expect("true block");
        block.instructions.push(LirInst::Const64 {
            dst: true_value,
            imm: 111,
            provenance: provenance.clone(),
        });
        block.terminator = LirTerminator::Ret {
            val: Some(true_value),
            provenance: provenance.clone(),
        };
    }

    {
        let block = function.block_mut(false_block).expect("false block");
        block.instructions.push(LirInst::Const64 {
            dst: false_value,
            imm: 222,
            provenance: provenance.clone(),
        });
        block.terminator = LirTerminator::Ret {
            val: Some(false_value),
            provenance,
        };
    }

    let _ = cml::x86_opt::optimize_lir(
        &mut function,
        cml::x86_opt::LocalOptConfig::all_enabled(),
    );

    let entry_after = function.block(entry).expect("entry after optimize");
    assert!(
        matches!(&entry_after.terminator, LirTerminator::BranchCond { .. }),
        "branch must not fold from stale pre-DIV high=0 fact"
    );

    let items = cml::x86_lir::lir_to_machine_items(&function)
        .expect("optimized div-rem branch function must emit");
    let bytes = assemble_program(&items).expect("assemble div-rem branch witness");
    let result = NativeExecutable::load(&bytes).call();
    assert_eq!(
        result, 222,
        "20 % 6 = 2, so post-DIV high == 0 must be false"
    );
}


#[test]
fn optimizer_preserves_deliberate_divisor_copy_away_from_rax_alias() {
    let provenance = Provenance::new(None, "x86-divrem-divisor-copy");
    let mut function = LirFunction::new("divrem_divisor_copy", provenance.clone());

    let numerator = function.alloc_vreg();
    let low = function.alloc_vreg();
    let high = function.alloc_vreg();
    let divisor_temp = function.alloc_vreg();
    let result = function.alloc_vreg();

    let block = function.block_mut(function.entry).expect("entry block");
    block.instructions.push(LirInst::Const64 {
        dst: numerator,
        imm: 20,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Copy {
        dst: low,
        src: numerator,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Const64 {
        dst: high,
        imm: 0,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Copy {
        dst: divisor_temp,
        src: low,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::DivRem {
        low,
        high,
        divisor: divisor_temp,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Copy {
        dst: result,
        src: low,
        provenance: provenance.clone(),
    });
    block.terminator = LirTerminator::Ret {
        val: Some(result),
        provenance,
    };

    let _ = cml::x86_opt::optimize_lir(
        &mut function,
        cml::x86_opt::LocalOptConfig::all_enabled(),
    );

    let divisor_after = function.blocks[0]
        .instructions
        .iter()
        .find_map(|inst| match inst {
            LirInst::DivRem { divisor, .. } => Some(*divisor),
            _ => None,
        })
        .expect("DivRem survives optimization");
    assert_eq!(
        divisor_after, divisor_temp,
        "constraint-sensitive DIV must keep deliberate divisor temp identity"
    );

    let items = cml::x86_lir::lir_to_machine_items(&function)
        .expect("optimized divisor-copy witness must emit");
    let bytes = assemble_program(&items).expect("assemble divisor-copy witness");
    let actual = NativeExecutable::load(&bytes).call();
    assert_eq!(actual, 1, "20 / 20 quotient must remain 1");
}
