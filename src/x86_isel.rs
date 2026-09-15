//! Skylake-driven scalar instruction selection pass (#59 Phase A).
//!
//! # Architecture and Philosophy
//!
//! - **Profile/Cost Policy Driven**: Selects faster native instruction sequences only when
//!   semantically exact and mechanically beneficial on the target Intel Core i5-6400 (Skylake).
//! - **Immediate vs Register Selection**:
//!   - Zero register idiom: transforms `movabsq $0, %reg` (10 bytes, pipeline dependency)
//!     into `xorq %reg, %reg` (3 bytes, zero latency at Skylake register rename stage).
//!   - Immediate ALU: transforms multi-instruction sequence (`movabsq $imm, %scratch` followed by `addq %scratch, %reg`)
//!     into a single `addq $imm8, %reg` (4 bytes) or `addq $imm32, %reg` (7 bytes), saving 1 instruction
//!     and removing scratch register pressure.
//! - **LEA-Style Arithmetic Idiom**:
//!   - Transforms `dst = base + disp` (where `dst != base`) from a two-instruction sequence
//!     (`movq %base, %dst; addq $disp, %dst`) into a single `leaq disp(%base), %dst` instruction (4-7 bytes).
//!   - Preserves CPU condition flags (EFLAGS) and executes with 1-cycle latency on Skylake ports 1/5.
//! - **Strict Equivalence Evidence**: Every transform guarantees bit-for-bit equivalence with
//!   the baseline scalar path and upstream `my-lisp` evaluation oracle.
//!
//! # Українська документація (Ukrainian Documentation)
//!
//! Модуль реалізує вибір скалярних машинних інструкцій на основі вартості для процесора
//! Intel Core i5-6400 (Skylake). Він оптимізує завантаження нулів через `xorq`, додавання
//! констант через `AluImm8`/`AluImm32` та зміщення через інструкцію `leaq`.

use std::collections::HashMap;

use crate::machine_inst::{AluOp, MachineInst, MachineItem, Provenance, X86Reg};
use crate::x86_lir::{LirFunction, LirInst, LirTerminator, VReg};
use crate::x86_regalloc::{
    AllocLocation, RegAllocError, RegAllocPlan, SCRATCH_REG_A, SCRATCH_REG_B,
};

/// Configuration controlling scalar instruction selection idioms on Skylake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarIselConfig {
    /// Use `xorq %r, %r` instead of `movabsq $0, %r` for zero constant loading.
    pub prefer_zero_xor: bool,
    /// Use `AluImm8`/`AluImm32` instead of register loading when an operand is an immediate.
    pub prefer_alu_imm: bool,
    /// Use `leaq disp(%base), %dst` for non-destructive additions of base register and immediate displacement.
    pub prefer_lea_for_add: bool,
}

impl ScalarIselConfig {
    /// Default configuration tuned for Intel Core i5-6400 (Skylake).
    pub fn default_skylake() -> Self {
        Self {
            prefer_zero_xor: true,
            prefer_alu_imm: true,
            prefer_lea_for_add: true,
        }
    }

    /// Reference configuration with all instruction selection idioms disabled.
    pub fn baseline_off() -> Self {
        Self {
            prefer_zero_xor: false,
            prefer_alu_imm: false,
            prefer_lea_for_add: false,
        }
    }
}

/// Emits machine items for an `LirFunction` using an explicit `RegAllocPlan` and `ScalarIselConfig`.
pub fn emit_machine_items_with_isel(
    func: &LirFunction,
    plan: &RegAllocPlan,
    config: &ScalarIselConfig,
) -> Result<Vec<MachineItem>, RegAllocError> {
    let mut items = Vec::new();
    let prov = Provenance::new(Some("0104"), "emit_machine_items_with_isel");

    // Gather map of known integer constants for immediate selection
    let mut known_consts: HashMap<VReg, i64> = HashMap::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let LirInst::Const64 { dst, imm, .. } = inst {
                known_consts.insert(*dst, *imm as i64);
            }
        }
    }

    // Frame size for spill slots: 16-byte aligned
    let frame_size = if plan.spill_count > 0 {
        ((plan.spill_count * 8 + 15) / 16) * 16
    } else {
        0
    };

    let get_loc = |vreg: VReg| -> Result<AllocLocation, RegAllocError> {
        plan.assignments.get(&vreg).copied().ok_or_else(|| {
            RegAllocError::Internal(format!("no allocation location found for {vreg}"))
        })
    };

    for (b_idx, block) in func.blocks.iter().enumerate() {
        if block.id != func.entry {
            items.push(MachineItem::Label(format!("{}", block.id)));
        }

        // Function prologue at entry block
        if b_idx == 0 && frame_size > 0 {
            items.push(MachineItem::Inst(MachineInst::AluImm32 {
                op: AluOp::Sub,
                dst: X86Reg::Rsp,
                imm: frame_size as i32,
                provenance: prov.clone(),
            }));
        }

        for inst in &block.instructions {
            match inst {
                LirInst::Const64 {
                    dst,
                    imm,
                    provenance,
                } => {
                    let loc = get_loc(*dst)?;
                    match loc {
                        AllocLocation::Reg(r) => {
                            // Phase A Idiom: xorq %r, %r for zero on Skylake
                            if *imm == 0 && config.prefer_zero_xor {
                                items.push(MachineItem::Inst(MachineInst::AluRegReg {
                                    op: AluOp::Xor,
                                    dst: r,
                                    src: r,
                                    provenance: provenance.clone(),
                                }));
                            } else {
                                items.push(MachineItem::Inst(MachineInst::MovImm64 {
                                    dst: r,
                                    imm: *imm,
                                    provenance: provenance.clone(),
                                }));
                            }
                        }
                        AllocLocation::SpillSlot(slot) => {
                            if *imm == 0 && config.prefer_zero_xor {
                                items.push(MachineItem::Inst(MachineInst::AluRegReg {
                                    op: AluOp::Xor,
                                    dst: SCRATCH_REG_A,
                                    src: SCRATCH_REG_A,
                                    provenance: provenance.clone(),
                                }));
                            } else {
                                items.push(MachineItem::Inst(MachineInst::MovImm64 {
                                    dst: SCRATCH_REG_A,
                                    imm: *imm,
                                    provenance: provenance.clone(),
                                }));
                            }
                            items.push(MachineItem::Inst(MachineInst::MovStore {
                                base: X86Reg::Rsp,
                                disp: (slot * 8) as i32,
                                src: SCRATCH_REG_A,
                                provenance: provenance.clone(),
                            }));
                        }
                    }
                }
                LirInst::Copy {
                    dst,
                    src,
                    provenance,
                } => {
                    let loc_dst = get_loc(*dst)?;
                    let loc_src = get_loc(*src)?;
                    match (loc_dst, loc_src) {
                        (AllocLocation::Reg(d), AllocLocation::Reg(s)) => {
                            if d != s {
                                items.push(MachineItem::Inst(MachineInst::MovRegReg {
                                    dst: d,
                                    src: s,
                                    provenance: provenance.clone(),
                                }));
                            }
                        }
                        (AllocLocation::Reg(d), AllocLocation::SpillSlot(s_slot)) => {
                            items.push(MachineItem::Inst(MachineInst::MovLoad {
                                dst: d,
                                base: X86Reg::Rsp,
                                disp: (s_slot * 8) as i32,
                                provenance: provenance.clone(),
                            }));
                        }
                        (AllocLocation::SpillSlot(d_slot), AllocLocation::Reg(s)) => {
                            items.push(MachineItem::Inst(MachineInst::MovStore {
                                base: X86Reg::Rsp,
                                disp: (d_slot * 8) as i32,
                                src: s,
                                provenance: provenance.clone(),
                            }));
                        }
                        (AllocLocation::SpillSlot(d_slot), AllocLocation::SpillSlot(s_slot)) => {
                            if d_slot != s_slot {
                                items.push(MachineItem::Inst(MachineInst::MovLoad {
                                    dst: SCRATCH_REG_A,
                                    base: X86Reg::Rsp,
                                    disp: (s_slot * 8) as i32,
                                    provenance: provenance.clone(),
                                }));
                                items.push(MachineItem::Inst(MachineInst::MovStore {
                                    base: X86Reg::Rsp,
                                    disp: (d_slot * 8) as i32,
                                    src: SCRATCH_REG_A,
                                    provenance: provenance.clone(),
                                }));
                            }
                        }
                    }
                }
                LirInst::Alu {
                    op,
                    dst,
                    lhs,
                    rhs,
                    provenance,
                } => {
                    let loc_dst = get_loc(*dst)?;
                    let loc_lhs = get_loc(*lhs)?;
                    let rhs_const = known_consts.get(rhs).copied();

                    // Phase A Idiom: LEA for `dst = lhs + imm` when dst != lhs and imm fits in i32
                    if config.prefer_lea_for_add
                        && *op == crate::x86_lir::LirAluOp::Add
                        && rhs_const.is_some()
                    {
                        let imm = rhs_const.unwrap();
                        if (i32::MIN as i64..=i32::MAX as i64).contains(&imm) {
                            if let (AllocLocation::Reg(d), AllocLocation::Reg(l)) =
                                (loc_dst, loc_lhs)
                            {
                                if d != l {
                                    items.push(MachineItem::Inst(MachineInst::Lea {
                                        dst: d,
                                        base: l,
                                        disp: imm as i32,
                                        provenance: provenance.clone(),
                                    }));
                                    continue;
                                }
                            }
                        }
                    }

                    // Resolve physical register for lhs
                    let (phys_lhs, need_store_dst) = match loc_dst {
                        AllocLocation::Reg(d) => {
                            match loc_lhs {
                                AllocLocation::Reg(l) => {
                                    if d != l {
                                        items.push(MachineItem::Inst(MachineInst::MovRegReg {
                                            dst: d,
                                            src: l,
                                            provenance: provenance.clone(),
                                        }));
                                    }
                                }
                                AllocLocation::SpillSlot(s) => {
                                    items.push(MachineItem::Inst(MachineInst::MovLoad {
                                        dst: d,
                                        base: X86Reg::Rsp,
                                        disp: (s * 8) as i32,
                                        provenance: provenance.clone(),
                                    }));
                                }
                            }
                            (d, None)
                        }
                        AllocLocation::SpillSlot(dst_slot) => {
                            match loc_lhs {
                                AllocLocation::Reg(l) => {
                                    items.push(MachineItem::Inst(MachineInst::MovRegReg {
                                        dst: SCRATCH_REG_A,
                                        src: l,
                                        provenance: provenance.clone(),
                                    }));
                                }
                                AllocLocation::SpillSlot(s) => {
                                    items.push(MachineItem::Inst(MachineInst::MovLoad {
                                        dst: SCRATCH_REG_A,
                                        base: X86Reg::Rsp,
                                        disp: (s * 8) as i32,
                                        provenance: provenance.clone(),
                                    }));
                                }
                            }
                            (SCRATCH_REG_A, Some(dst_slot))
                        }
                    };

                    // Phase A Idiom: Immediate ALU operation if rhs is a known constant
                    let emitted_imm_alu = if config.prefer_alu_imm && rhs_const.is_some() {
                        let imm = rhs_const.unwrap();
                        let alu_op = op.to_x86_alu();
                        if (-128..=127).contains(&imm) {
                            items.push(MachineItem::Inst(MachineInst::AluImm8 {
                                op: alu_op,
                                dst: phys_lhs,
                                imm: imm as i8,
                                provenance: provenance.clone(),
                            }));
                            true
                        } else if (i32::MIN as i64..=i32::MAX as i64).contains(&imm) {
                            items.push(MachineItem::Inst(MachineInst::AluImm32 {
                                op: alu_op,
                                dst: phys_lhs,
                                imm: imm as i32,
                                provenance: provenance.clone(),
                            }));
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !emitted_imm_alu {
                        // Fallback: resolve physical register for rhs and emit AluRegReg
                        let loc_rhs = get_loc(*rhs)?;
                        let phys_rhs = match loc_rhs {
                            AllocLocation::Reg(r) => r,
                            AllocLocation::SpillSlot(s) => {
                                items.push(MachineItem::Inst(MachineInst::MovLoad {
                                    dst: SCRATCH_REG_B,
                                    base: X86Reg::Rsp,
                                    disp: (s * 8) as i32,
                                    provenance: provenance.clone(),
                                }));
                                SCRATCH_REG_B
                            }
                        };

                        items.push(MachineItem::Inst(MachineInst::AluRegReg {
                            op: op.to_x86_alu(),
                            dst: phys_lhs,
                            src: phys_rhs,
                            provenance: provenance.clone(),
                        }));
                    }

                    if let Some(slot) = need_store_dst {
                        items.push(MachineItem::Inst(MachineInst::MovStore {
                            base: X86Reg::Rsp,
                            disp: (slot * 8) as i32,
                            src: SCRATCH_REG_A,
                            provenance: provenance.clone(),
                        }));
                    }
                }
                LirInst::Cmp {
                    lhs,
                    rhs,
                    provenance,
                } => {
                    let loc_lhs = get_loc(*lhs)?;
                    let loc_rhs = get_loc(*rhs)?;

                    let phys_lhs = match loc_lhs {
                        AllocLocation::Reg(r) => r,
                        AllocLocation::SpillSlot(s) => {
                            items.push(MachineItem::Inst(MachineInst::MovLoad {
                                dst: SCRATCH_REG_A,
                                base: X86Reg::Rsp,
                                disp: (s * 8) as i32,
                                provenance: provenance.clone(),
                            }));
                            SCRATCH_REG_A
                        }
                    };

                    let phys_rhs = match loc_rhs {
                        AllocLocation::Reg(r) => r,
                        AllocLocation::SpillSlot(s) => {
                            items.push(MachineItem::Inst(MachineInst::MovLoad {
                                dst: SCRATCH_REG_B,
                                base: X86Reg::Rsp,
                                disp: (s * 8) as i32,
                                provenance: provenance.clone(),
                            }));
                            SCRATCH_REG_B
                        }
                    };

                    items.push(MachineItem::Inst(MachineInst::AluRegReg {
                        op: AluOp::Cmp,
                        dst: phys_lhs,
                        src: phys_rhs,
                        provenance: provenance.clone(),
                    }));
                }
                LirInst::UnboxFixnum {
                    dst,
                    src,
                    provenance,
                } => {
                    let loc_dst = get_loc(*dst)?;
                    let loc_src = get_loc(*src)?;

                    let (work_reg, dst_slot) = match loc_dst {
                        AllocLocation::Reg(r) => (r, None),
                        AllocLocation::SpillSlot(s) => (SCRATCH_REG_A, Some(s)),
                    };

                    match loc_src {
                        AllocLocation::Reg(r) => {
                            if work_reg != r {
                                items.push(MachineItem::Inst(MachineInst::MovRegReg {
                                    dst: work_reg,
                                    src: r,
                                    provenance: provenance.clone(),
                                }));
                            }
                        }
                        AllocLocation::SpillSlot(s) => {
                            items.push(MachineItem::Inst(MachineInst::MovLoad {
                                dst: work_reg,
                                base: X86Reg::Rsp,
                                disp: (s * 8) as i32,
                                provenance: provenance.clone(),
                            }));
                        }
                    }

                    items.push(MachineItem::Inst(MachineInst::SarImm {
                        reg: work_reg,
                        imm: 3,
                        provenance: provenance.clone(),
                    }));

                    if let Some(slot) = dst_slot {
                        items.push(MachineItem::Inst(MachineInst::MovStore {
                            base: X86Reg::Rsp,
                            disp: (slot * 8) as i32,
                            src: work_reg,
                            provenance: provenance.clone(),
                        }));
                    }
                }
                LirInst::BoxFixnum {
                    dst,
                    src,
                    provenance,
                } => {
                    let loc_dst = get_loc(*dst)?;
                    let loc_src = get_loc(*src)?;

                    let (work_reg, dst_slot) = match loc_dst {
                        AllocLocation::Reg(r) => (r, None),
                        AllocLocation::SpillSlot(s) => (SCRATCH_REG_A, Some(s)),
                    };

                    match loc_src {
                        AllocLocation::Reg(r) => {
                            if work_reg != r {
                                items.push(MachineItem::Inst(MachineInst::MovRegReg {
                                    dst: work_reg,
                                    src: r,
                                    provenance: provenance.clone(),
                                }));
                            }
                        }
                        AllocLocation::SpillSlot(s) => {
                            items.push(MachineItem::Inst(MachineInst::MovLoad {
                                dst: work_reg,
                                base: X86Reg::Rsp,
                                disp: (s * 8) as i32,
                                provenance: provenance.clone(),
                            }));
                        }
                    }

                    items.push(MachineItem::Inst(MachineInst::ShlImm {
                        reg: work_reg,
                        imm: 3,
                        provenance: provenance.clone(),
                    }));
                    items.push(MachineItem::Inst(MachineInst::AluImm8 {
                        op: AluOp::Or,
                        dst: work_reg,
                        imm: wsm_os_target::Tag::Fixnum as i8,
                        provenance: provenance.clone(),
                    }));

                    if let Some(slot) = dst_slot {
                        items.push(MachineItem::Inst(MachineInst::MovStore {
                            base: X86Reg::Rsp,
                            disp: (slot * 8) as i32,
                            src: work_reg,
                            provenance: provenance.clone(),
                        }));
                    }
                }
            }
        }

        // Block terminator
        match &block.terminator {
            LirTerminator::Jmp { target, provenance } => {
                items.push(MachineItem::JmpLabel {
                    target: format!("{target}"),
                    provenance: provenance.clone(),
                });
            }
            LirTerminator::BranchCond {
                cond,
                true_block,
                false_block,
                provenance,
            } => {
                items.push(MachineItem::JccLabel {
                    cond: cond.to_x86_cond(),
                    target: format!("{true_block}"),
                    provenance: provenance.clone(),
                });
                items.push(MachineItem::JmpLabel {
                    target: format!("{false_block}"),
                    provenance: provenance.clone(),
                });
            }
            LirTerminator::Ret { val, provenance } => {
                if let Some(v) = val {
                    let loc = get_loc(*v)?;
                    match loc {
                        AllocLocation::Reg(r) => {
                            if r != X86Reg::Rax {
                                items.push(MachineItem::Inst(MachineInst::MovRegReg {
                                    dst: X86Reg::Rax,
                                    src: r,
                                    provenance: provenance.clone(),
                                }));
                            }
                        }
                        AllocLocation::SpillSlot(s) => {
                            items.push(MachineItem::Inst(MachineInst::MovLoad {
                                dst: X86Reg::Rax,
                                base: X86Reg::Rsp,
                                disp: (s * 8) as i32,
                                provenance: provenance.clone(),
                            }));
                        }
                    }
                }

                // Epilogue: deallocate spill frame if present
                if frame_size > 0 {
                    items.push(MachineItem::Inst(MachineInst::AluImm32 {
                        op: AluOp::Add,
                        dst: X86Reg::Rsp,
                        imm: frame_size as i32,
                        provenance: provenance.clone(),
                    }));
                }

                items.push(MachineItem::Inst(MachineInst::Ret {
                    provenance: provenance.clone(),
                }));
            }
        }
    }

    Ok(items)
}

/// Convenience function to allocate registers and emit machine items using scalar instruction selection.
pub fn lir_to_machine_items_with_isel(
    func: &LirFunction,
    config: &ScalarIselConfig,
) -> Result<Vec<MachineItem>, crate::x86_lir::LirEmitError> {
    let plan = crate::x86_regalloc::allocate_registers(func)
        .map_err(|e| crate::x86_lir::LirEmitError::RegisterExhaustion(format!("{e}")))?;
    emit_machine_items_with_isel(func, &plan, config)
        .map_err(|e| crate::x86_lir::LirEmitError::RegisterExhaustion(format!("{e}")))
}
