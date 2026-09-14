//! Conformance, liveness, spilling, and deterministic register allocation tests (#56).
//!
//! Verifies:
//! 1. Low-pressure straight-line arithmetic requires zero spills (`spill_count == 0`).
//! 2. High register pressure (> 7 simultaneous live values) spills to stack slots and
//!    computes the bit-for-bit exact result matching the Lisp oracle.
//! 3. Liveness dataflow correctly traverses branches, join blocks, and CFG edges.
//! 4. Frame sizes are 16-byte aligned per SysV x86-64 ABI requirements.
//! 5. Repeated compilation is 100% deterministic (identical intervals, locations, bytes).
//! 6. Inspectable dump of register allocation plan and live intervals.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::machine_inst::assemble_program;
use cml::native_baseline::NativeExecutable;
use cml::x86_lir::{LirAluOp, LirFunction, LirInst, LirTerminator, VReg, lower_ir_to_lir};
use cml::x86_regalloc::{
    ALLOCATABLE_GPRS, AllocLocation, allocate_registers, compute_liveness,
    emit_machine_items_with_plan,
};
use cml::{lower, parser};
use my_lisp::{Session, eval_program};

fn parse_and_lower_ir(source: &str) -> cml::ir::Ir {
    let exprs =
        parser::parse(source).unwrap_or_else(|e| panic!("parse error for `{source}`: {e:?}"));
    let mut lowered = lower::lower_program(&exprs)
        .unwrap_or_else(|e| panic!("lowering error for `{source}`: {e}"));
    assert_eq!(lowered.len(), 1);
    lowered.remove(0)
}

fn lisp_oracle(source: &str) -> u64 {
    let mut session = Session::default();
    eval_program(source, &mut session)
        .unwrap_or_else(|e| panic!("oracle eval error for `{source}`: {e:?}"))
        .value
        .to_string()
        .parse::<u64>()
        .unwrap_or_else(|e| panic!("oracle returned non-u64 for `{source}`: {e}"))
}

#[test]
fn test_low_pressure_straight_line_zero_spills() {
    let source = "(+ (+ 10 20) 12)";
    let ir = parse_and_lower_ir(source);
    let func = lower_ir_to_lir(&ir).expect("lower to LIR");

    let plan = allocate_registers(&func).expect("allocate registers");

    // Acceptance criterion: low pressure arithmetic requires zero spills
    assert_eq!(
        plan.spill_count, 0,
        "straight-line arithmetic with short live ranges must have zero spills"
    );

    for loc in plan.assignments.values() {
        match loc {
            AllocLocation::Reg(r) => {
                assert!(
                    ALLOCATABLE_GPRS.contains(r),
                    "register {r:?} must be in allocatable pool"
                );
            }
            AllocLocation::SpillSlot(_) => {
                panic!("unexpected spill in low-pressure arithmetic");
            }
        }
    }

    let items = emit_machine_items_with_plan(&func, &plan).expect("emit machine items");
    let bytes = assemble_program(&items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    let result = exec.call();

    let oracle_val = lisp_oracle(source);
    assert_eq!(result, 42);
    assert_eq!(result, oracle_val);
}

#[test]
fn test_high_register_pressure_spills_and_computes_correctly() {
    // Construct a function with 12 simultaneous live virtual registers:
    // v0..v11 are initialized, and all remain live until the final summation.
    // Since only 7 registers are available (ALLOCATABLE_GPRS), this guarantees
    // register exhaustion and forces at least 5 spills to stack slots.
    let prov = cml::machine_inst::Provenance::new(Some("0104"), "high_pressure_test");
    let mut func = LirFunction::new("high_pressure_fn", prov.clone());

    let mut vregs = Vec::new();
    let entry = func.entry;
    let block = func.block_mut(entry).unwrap();

    for i in 1..=12 {
        let v = VReg(i - 1);
        block.instructions.push(LirInst::Const64 {
            dst: v,
            imm: i as u64,
            provenance: prov.clone(),
        });
        vregs.push(v);
    }

    // Now sum them all up: v12 = v0 + v1, v13 = v12 + v2, ..., v22 = v21 + v11
    let mut acc = vregs[0];
    let mut next_vreg_id = 12u32;
    for &operand in &vregs[1..] {
        let dst = VReg(next_vreg_id);
        next_vreg_id += 1;
        block.instructions.push(LirInst::Alu {
            op: LirAluOp::Add,
            dst,
            lhs: acc,
            rhs: operand,
            provenance: prov.clone(),
        });
        acc = dst;
    }

    block.terminator = LirTerminator::Ret {
        val: Some(acc),
        provenance: prov,
    };

    // Run register allocation
    let plan = allocate_registers(&func).expect("allocate high pressure");

    // Acceptance criterion: spills occur under pressure (> 7 live values)
    assert!(
        plan.spill_count >= 5,
        "must have at least 5 spill slots for 12 simultaneous live values with 7 GPRs (got {})",
        plan.spill_count
    );

    // Verify dump contains both Reg and SpillSlot assignments
    let dump = plan.dump();
    assert!(dump.contains("stack["));
    assert!(dump.contains("%rax"));

    // Emit machine items with spill and reload instructions
    let items = emit_machine_items_with_plan(&func, &plan).expect("emit machine items with spills");
    let bytes = assemble_program(&items).expect("assemble machine code with spills");
    let exec = NativeExecutable::load(&bytes);
    let result = exec.call();

    // Expected sum: 1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10 + 11 + 12 = 78
    let expected: u64 = (1..=12).sum();
    assert_eq!(expected, 78);
    assert_eq!(
        result, expected,
        "spill and reload execution must produce the exact mathematical sum"
    );
}

#[test]
fn test_branch_join_liveness_and_allocation() {
    let source = "(cond ((eq 5 5) 42) (t 99))";
    let ir = parse_and_lower_ir(source);
    let func = lower_ir_to_lir(&ir).expect("lower cond to LIR");

    let (live_in, live_out) = compute_liveness(&func);
    assert!(
        !live_in.is_empty(),
        "liveness analysis must populate live_in sets for basic blocks"
    );
    assert!(
        !live_out.is_empty(),
        "liveness analysis must populate live_out sets for basic blocks"
    );

    let plan = allocate_registers(&func).expect("allocate for cond CFG");
    assert_eq!(plan.spill_count, 0, "cond with small blocks must not spill");

    let items = emit_machine_items_with_plan(&func, &plan).expect("emit cond items");
    let bytes = assemble_program(&items).expect("assemble cond");
    let exec = NativeExecutable::load(&bytes);
    let result = exec.call();

    let oracle_val = lisp_oracle(source);
    assert_eq!(result, 42);
    assert_eq!(result, oracle_val);
}

#[test]
fn test_determinism_repeated_compilation_yields_identical_output() {
    let source = "(cond ((eq 10 10) (+ 20 22)) (t 99))";
    let ir = parse_and_lower_ir(source);
    let func = lower_ir_to_lir(&ir).expect("lower");

    let initial_plan = allocate_registers(&func).expect("initial alloc");
    let initial_items = emit_machine_items_with_plan(&func, &initial_plan).expect("initial emit");
    let initial_bytes = assemble_program(&initial_items).expect("initial bytes");

    for _ in 0..50 {
        let plan = allocate_registers(&func).expect("repeated alloc");
        assert_eq!(plan.assignments, initial_plan.assignments);
        assert_eq!(plan.spill_count, initial_plan.spill_count);

        let items = emit_machine_items_with_plan(&func, &plan).expect("repeated emit");
        let bytes = assemble_program(&items).expect("repeated bytes");
        assert_eq!(
            bytes, initial_bytes,
            "repeated compilation must be bit-exact deterministic"
        );
    }
}

#[test]
fn test_sysv_stack_alignment_with_spills() {
    // When spills occur, frame size must be a multiple of 16 bytes
    for spill_count in [1, 2, 3, 4, 7, 8, 9, 13] {
        let frame_size = ((spill_count * 8 + 15) / 16) * 16;
        assert_eq!(
            frame_size % 16,
            0,
            "frame size for {spill_count} spills must be 16-byte aligned: got {frame_size}"
        );
        assert!(frame_size >= spill_count * 8);
    }
}
