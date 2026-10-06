//! Backend-local fixed-register constraint tests for cml#523.
//!
//! These tests exercise allocator mechanism only. Shared CML IR and SENS
//! semantic identity remain register-free.

use cml::machine_inst::{Provenance, X86Reg};
use cml::x86_lir::{LirAluOp, LirFunction, LirInst, LirTerminator, VReg};
use cml::x86_regalloc::{
    AllocLocation, FixedRegConstraint, RegAllocError, allocate_registers,
    allocate_registers_with_constraints, unsigned_dividend_constraints,
};

fn overlapping_three_value_function() -> LirFunction {
    let provenance = Provenance::new(Some("0104"), "fixed-reg-constraint-test");
    let mut function = LirFunction::new("fixed_reg_test", provenance.clone());
    let block = function
        .block_mut(function.entry)
        .expect("entry block must exist");

    let left = VReg(0);
    let right = VReg(1);
    let sum = VReg(2);

    block.instructions.push(LirInst::Const64 {
        dst: left,
        imm: 20,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Const64 {
        dst: right,
        imm: 22,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Alu {
        op: LirAluOp::Add,
        dst: sum,
        lhs: left,
        rhs: right,
        provenance: provenance.clone(),
    });
    block.terminator = LirTerminator::Ret {
        val: Some(sum),
        provenance,
    };

    function
}


fn non_overlapping_two_value_function() -> LirFunction {
    let provenance = Provenance::new(Some("0104"), "fixed-reg-reuse-test");
    let mut function = LirFunction::new("fixed_reg_reuse", provenance.clone());
    let block = function
        .block_mut(function.entry)
        .expect("entry block must exist");

    let first = VReg(0);
    let first_use = VReg(2);
    let second = VReg(1);

    block.instructions.push(LirInst::Const64 {
        dst: first,
        imm: 20,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Alu {
        op: LirAluOp::Add,
        dst: first_use,
        lhs: first,
        rhs: first,
        provenance: provenance.clone(),
    });
    block.instructions.push(LirInst::Const64 {
        dst: second,
        imm: 22,
        provenance: provenance.clone(),
    });
    block.terminator = LirTerminator::Ret {
        val: Some(second),
        provenance,
    };

    function
}

#[test]
fn empty_constraint_set_is_exactly_legacy_allocation() {
    let function = overlapping_three_value_function();

    let legacy = allocate_registers(&function).expect("legacy allocation");
    let explicit_empty =
        allocate_registers_with_constraints(&function, &[]).expect("empty constrained allocation");

    assert_eq!(legacy, explicit_empty);
    assert!(legacy.fixed_constraints.is_empty());
}

#[test]
fn fixed_interval_takes_required_register_and_spills_ordinary_owner() {
    let function = overlapping_three_value_function();

    let plan = allocate_registers_with_constraints(
        &function,
        &[FixedRegConstraint::new(VReg(1), X86Reg::Rax)],
    )
    .expect("fixed register must be allocatable");

    assert_eq!(
        plan.assignments.get(&VReg(1)),
        Some(&AllocLocation::Reg(X86Reg::Rax))
    );
    assert_eq!(plan.fixed_constraints.get(&VReg(1)), Some(&X86Reg::Rax));

    // v0 starts first and takes RAX on the ordinary linear-scan route. When
    // overlapping fixed v1 arrives it must evict/spill v0 rather than silently
    // place v1 in another register.
    assert!(matches!(
        plan.assignments.get(&VReg(0)),
        Some(AllocLocation::SpillSlot(_))
    ));
    assert_eq!(plan.spill_count, 1);
    assert!(plan.dump().contains("must=%rax"));
}

#[test]
fn unsigned_dividend_recipe_binds_low_to_rax_and_high_to_rdx() {
    let constraints = unsigned_dividend_constraints(VReg(10), VReg(11));

    assert_eq!(
        constraints,
        [
            FixedRegConstraint::new(VReg(10), X86Reg::Rax),
            FixedRegConstraint::new(VReg(11), X86Reg::Rdx),
        ]
    );
}


#[test]
fn same_fixed_register_can_be_reused_after_live_range_expires() {
    let function = non_overlapping_two_value_function();

    let plan = allocate_registers_with_constraints(
        &function,
        &[
            FixedRegConstraint::new(VReg(0), X86Reg::Rax),
            FixedRegConstraint::new(VReg(1), X86Reg::Rax),
        ],
    )
    .expect("non-overlapping fixed intervals may reuse one physical register");

    assert_eq!(
        plan.assignments.get(&VReg(0)),
        Some(&AllocLocation::Reg(X86Reg::Rax))
    );
    assert_eq!(
        plan.assignments.get(&VReg(1)),
        Some(&AllocLocation::Reg(X86Reg::Rax))
    );
    assert_eq!(plan.spill_count, 0);
}

#[test]
fn overlapping_fixed_intervals_cannot_claim_the_same_register() {
    let function = overlapping_three_value_function();

    let error = allocate_registers_with_constraints(
        &function,
        &[
            FixedRegConstraint::new(VReg(0), X86Reg::Rax),
            FixedRegConstraint::new(VReg(1), X86Reg::Rax),
        ],
    )
    .expect_err("overlapping fixed live ranges must fail closed");

    assert_eq!(
        error,
        RegAllocError::OverlappingFixedRegister {
            reg: X86Reg::Rax,
            first: VReg(0),
            second: VReg(1),
        }
    );
}

#[test]
fn unknown_vreg_constraint_fails_closed() {
    let function = overlapping_three_value_function();

    assert_eq!(
        allocate_registers_with_constraints(
            &function,
            &[FixedRegConstraint::new(VReg(99), X86Reg::Rax)],
        ),
        Err(RegAllocError::UnknownConstrainedVReg { vreg: VReg(99) })
    );
}

#[test]
fn non_allocatable_scratch_register_cannot_be_requested() {
    let function = overlapping_three_value_function();

    assert_eq!(
        allocate_registers_with_constraints(
            &function,
            &[FixedRegConstraint::new(VReg(0), X86Reg::R10)],
        ),
        Err(RegAllocError::UnavailableFixedRegister {
            vreg: VReg(0),
            reg: X86Reg::R10,
        })
    );
}

#[test]
fn duplicate_identical_constraint_is_idempotent() {
    let function = overlapping_three_value_function();
    let constraint = FixedRegConstraint::new(VReg(0), X86Reg::Rdx);

    let once = allocate_registers_with_constraints(&function, &[constraint])
        .expect("single fixed requirement");
    let twice = allocate_registers_with_constraints(&function, &[constraint, constraint])
        .expect("identical duplicate must not change the plan");

    assert_eq!(twice, once);
    assert_eq!(twice.fixed_constraints.get(&VReg(0)), Some(&X86Reg::Rdx));
}

#[test]
fn contradictory_constraints_for_one_vreg_fail_closed() {
    let function = overlapping_three_value_function();

    assert_eq!(
        allocate_registers_with_constraints(
            &function,
            &[
                FixedRegConstraint::new(VReg(0), X86Reg::Rax),
                FixedRegConstraint::new(VReg(0), X86Reg::Rdx),
            ],
        ),
        Err(RegAllocError::ConflictingConstraint {
            vreg: VReg(0),
            first: X86Reg::Rax,
            second: X86Reg::Rdx,
        })
    );
}

#[test]
fn repeated_constrained_allocation_is_deterministic() {
    let function = overlapping_three_value_function();
    let constraints = [
        FixedRegConstraint::new(VReg(0), X86Reg::Rdx),
        FixedRegConstraint::new(VReg(2), X86Reg::Rax),
    ];

    let first =
        allocate_registers_with_constraints(&function, &constraints).expect("first allocation");

    for _ in 0..50 {
        let repeated = allocate_registers_with_constraints(&function, &constraints)
            .expect("repeated allocation");
        assert_eq!(repeated, first);
    }
}
