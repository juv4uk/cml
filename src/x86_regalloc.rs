//! Deterministic Liveness Analysis and Linear Scan Register Allocation for x86 LIR (#56).
//!
//! # Architecture & Scope (ADR-004 & Issue #56)
//!
//! Under ADR-004 and the Native Performance Roadmap (#51), this module replaces fixed/manual
//! register choices with a deterministic register allocator over x86 Lowered IR (`LirFunction`).
//!
//! ## Authority & Invariants
//! - `my-lisp` owns language semantics, numeric types, and evaluation contracts.
//! - Shared `Ir` (`src/ir.rs`) remains strictly untouched (zero register hints or spill variants).
//! - All liveness sets, intervals, physical register assignments, and spill slot insertions
//!   live exclusively in this backend-local module.
//!
//! ## Allocation Pipeline
//! 1. **Liveness Analysis**: Iterative backward dataflow over basic blocks (`live_in`, `live_out`).
//! 2. **Interval Construction**: Deterministic live intervals `[start, end]` per virtual register.
//! 3. **Linear Scan Allocation**:
//!    - Allocatable pool: caller-saved GPRs (`Rax, Rcx, Rdx, Rsi, Rdi, R8, R9`).
//!    - Dedicated scratch registers: `R10, R11` for spill reloads and stores.
//!    - Reserved registers: `Rsp` (stack pointer).
//! 4. **Spill/Reload Code Generation**: When register pressure exceeds available GPRs,
//!    variables are assigned 16-byte-aligned stack frame slots, with explicit reloads/stores.

use crate::machine_inst::{AluOp, MachineInst, MachineItem, Provenance, X86Reg};
use crate::x86_lir::{BlockId, LirFunction, LirInst, LirTerminator, VReg};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Allocatable caller-saved physical general-purpose registers (7 registers).
pub const ALLOCATABLE_GPRS: [X86Reg; 7] = [
    X86Reg::Rax,
    X86Reg::Rcx,
    X86Reg::Rdx,
    X86Reg::Rsi,
    X86Reg::Rdi,
    X86Reg::R8,
    X86Reg::R9,
];

/// Dedicated scratch registers for spill slot reloads and stores.
pub const SCRATCH_REG_A: X86Reg = X86Reg::R10;
pub const SCRATCH_REG_B: X86Reg = X86Reg::R11;

/// Physical location assigned to a virtual register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AllocLocation {
    /// Allocated to a physical CPU register.
    Reg(X86Reg),
    /// Spilled to a stack frame slot (0-indexed quadword, relative to `%rsp`).
    SpillSlot(u32),
}

impl fmt::Display for AllocLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reg(r) => write!(f, "{}", r.name()),
            Self::SpillSlot(s) => write!(f, "stack[{s}]"),
        }
    }
}

/// Live interval representing the lifetime of a virtual register in program points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveInterval {
    pub vreg: VReg,
    pub start: usize,
    pub end: usize,
}

/// Results and provenance of the register allocation pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegAllocPlan {
    pub assignments: HashMap<VReg, AllocLocation>,
    pub spill_count: usize,
    pub intervals: HashMap<VReg, LiveInterval>,
}

impl RegAllocPlan {
    /// Human-readable, deterministic dump of allocation plan.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "RegAllocPlan (spill_count: {})\n",
            self.spill_count
        ));
        let mut sorted_vregs: Vec<_> = self.assignments.keys().cloned().collect();
        sorted_vregs.sort_by_key(|v| v.0);
        for vreg in sorted_vregs {
            let loc = &self.assignments[&vreg];
            let interval = self.intervals.get(&vreg);
            if let Some(inv) = interval {
                out.push_str(&format!(
                    "  {}: {} (live: {}..={})\n",
                    vreg, loc, inv.start, inv.end
                ));
            } else {
                out.push_str(&format!("  {}: {}\n", vreg, loc));
            }
        }
        out
    }
}

/// Error encountered during register allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegAllocError {
    Internal(String),
}

impl fmt::Display for RegAllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Internal(msg) => write!(f, "regalloc internal error: {msg}"),
        }
    }
}

impl std::error::Error for RegAllocError {}

/// Computes use and def sets for a given instruction.
fn inst_vregs(inst: &LirInst) -> (Vec<VReg>, Vec<VReg>) {
    match inst {
        LirInst::Const64 { dst, .. } => (Vec::new(), vec![*dst]),
        LirInst::Copy { dst, src, .. } => (vec![*src], vec![*dst]),
        LirInst::Alu { dst, lhs, rhs, .. } => (vec![*lhs, *rhs], vec![*dst]),
        LirInst::Cmp { lhs, rhs, .. } => (vec![*lhs, *rhs], Vec::new()),
        LirInst::UnboxFixnum { dst, src, .. } => (vec![*src], vec![*dst]),
        LirInst::BoxFixnum { dst, src, .. } => (vec![*src], vec![*dst]),
    }
}

/// Computes use set for a terminator.
fn term_vregs(term: &LirTerminator) -> Vec<VReg> {
    match term {
        LirTerminator::Ret { val: Some(v), .. } => vec![*v],
        _ => Vec::new(),
    }
}

/// Computes backward liveness across all basic blocks in an `LirFunction`.
pub fn compute_liveness(
    func: &LirFunction,
) -> (
    HashMap<BlockId, HashSet<VReg>>,
    HashMap<BlockId, HashSet<VReg>>,
) {
    let mut block_use = HashMap::new();
    let mut block_def = HashMap::new();

    for block in &func.blocks {
        let mut b_use = HashSet::new();
        let mut b_def = HashSet::new();

        for inst in &block.instructions {
            let (uses, defs) = inst_vregs(inst);
            for u in uses {
                if !b_def.contains(&u) {
                    b_use.insert(u);
                }
            }
            for d in defs {
                b_def.insert(d);
            }
        }

        for u in term_vregs(&block.terminator) {
            if !b_def.contains(&u) {
                b_use.insert(u);
            }
        }

        block_use.insert(block.id, b_use);
        block_def.insert(block.id, b_def);
    }

    let mut live_in: HashMap<BlockId, HashSet<VReg>> = HashMap::new();
    let mut live_out: HashMap<BlockId, HashSet<VReg>> = HashMap::new();

    let mut changed = true;
    while changed {
        changed = false;
        for block in func.blocks.iter().rev() {
            let mut new_live_out = HashSet::new();
            for succ in block.terminator.successors() {
                if let Some(in_set) = live_in.get(&succ) {
                    new_live_out.extend(in_set);
                }
            }

            let mut new_live_in = block_use[&block.id].clone();
            for v in &new_live_out {
                if !block_def[&block.id].contains(v) {
                    new_live_in.insert(*v);
                }
            }

            if live_out.get(&block.id) != Some(&new_live_out) {
                live_out.insert(block.id, new_live_out);
                changed = true;
            }
            if live_in.get(&block.id) != Some(&new_live_in) {
                live_in.insert(block.id, new_live_in);
                changed = true;
            }
        }
    }

    (live_in, live_out)
}

/// Builds deterministic live intervals for all virtual registers.
pub fn build_live_intervals(func: &LirFunction) -> HashMap<VReg, LiveInterval> {
    let (live_in, live_out) = compute_liveness(func);

    let mut intervals: HashMap<VReg, (usize, usize)> = HashMap::new();
    let mut current_pos = 0;

    for block in &func.blocks {
        let block_start = current_pos;

        // VRegs live into block start at least at block_start
        if let Some(in_set) = live_in.get(&block.id) {
            for v in in_set {
                intervals
                    .entry(*v)
                    .and_modify(|(s, e)| {
                        *s = (*s).min(block_start);
                        *e = (*e).max(block_start);
                    })
                    .or_insert((block_start, block_start));
            }
        }

        for inst in &block.instructions {
            let (uses, defs) = inst_vregs(inst);
            for u in uses {
                intervals
                    .entry(u)
                    .and_modify(|(s, e)| {
                        *s = (*s).min(current_pos);
                        *e = (*e).max(current_pos);
                    })
                    .or_insert((current_pos, current_pos));
            }
            for d in defs {
                intervals
                    .entry(d)
                    .and_modify(|(s, e)| {
                        *s = (*s).min(current_pos);
                        *e = (*e).max(current_pos);
                    })
                    .or_insert((current_pos, current_pos));
            }
            current_pos += 2;
        }

        // Terminator uses
        for u in term_vregs(&block.terminator) {
            intervals
                .entry(u)
                .and_modify(|(s, e)| {
                    *s = (*s).min(current_pos);
                    *e = (*e).max(current_pos);
                })
                .or_insert((current_pos, current_pos));
        }

        // VRegs live out of block extend to block end
        if let Some(out_set) = live_out.get(&block.id) {
            for v in out_set {
                intervals
                    .entry(*v)
                    .and_modify(|(s, e)| {
                        *s = (*s).min(block_start);
                        *e = (*e).max(current_pos);
                    })
                    .or_insert((block_start, current_pos));
            }
        }

        current_pos += 2;
    }

    intervals
        .into_iter()
        .map(|(vreg, (start, end))| (vreg, LiveInterval { vreg, start, end }))
        .collect()
}

/// Allocates physical registers or stack spill slots using deterministic Linear Scan.
pub fn allocate_registers(func: &LirFunction) -> Result<RegAllocPlan, RegAllocError> {
    let intervals = build_live_intervals(func);

    let mut sorted_intervals: Vec<LiveInterval> = intervals.values().cloned().collect();
    // Deterministic sort: start position ascending, then vreg ID ascending
    sorted_intervals.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.vreg.0.cmp(&b.vreg.0)));

    let mut assignments: HashMap<VReg, AllocLocation> = HashMap::new();
    // Active intervals sorted by end position ascending
    let mut active: Vec<(LiveInterval, X86Reg)> = Vec::new();
    let mut free_pool: Vec<X86Reg> = ALLOCATABLE_GPRS.to_vec();

    let mut next_spill_slot = 0u32;

    for interval in sorted_intervals {
        // 1. Expire old intervals
        let mut i = 0;
        while i < active.len() {
            if active[i].0.end < interval.start {
                let freed_reg = active[i].1;
                active.remove(i);
                if !free_pool.contains(&freed_reg) {
                    free_pool.push(freed_reg);
                    // Keep free pool in canonical order for determinism
                    free_pool.sort_by_key(|r| ALLOCATABLE_GPRS.iter().position(|x| x == r));
                }
            } else {
                i += 1;
            }
        }

        // 2. Allocate or spill
        if !free_pool.is_empty() {
            let reg = free_pool.remove(0);
            assignments.insert(interval.vreg, AllocLocation::Reg(reg));
            active.push((interval, reg));
            active.sort_by_key(|(inv, _)| inv.end);
        } else {
            // Register pressure exceeded: spill interval with furthest end
            let last_idx = active.len() - 1;
            if active[last_idx].0.end > interval.end {
                // Spill candidate from active
                let (spilled_inv, stolen_reg) = active.remove(last_idx);
                let slot = next_spill_slot;
                next_spill_slot += 1;
                assignments.insert(spilled_inv.vreg, AllocLocation::SpillSlot(slot));

                assignments.insert(interval.vreg, AllocLocation::Reg(stolen_reg));
                active.push((interval, stolen_reg));
                active.sort_by_key(|(inv, _)| inv.end);
            } else {
                // Spill current interval
                let slot = next_spill_slot;
                next_spill_slot += 1;
                assignments.insert(interval.vreg, AllocLocation::SpillSlot(slot));
            }
        }
    }

    Ok(RegAllocPlan {
        assignments,
        spill_count: next_spill_slot as usize,
        intervals,
    })
}

/// Emits physical machine items for an `LirFunction` using an explicit `RegAllocPlan`.
pub fn emit_machine_items_with_plan(
    func: &LirFunction,
    plan: &RegAllocPlan,
) -> Result<Vec<MachineItem>, RegAllocError> {
    let mut items = Vec::new();
    let prov = Provenance::new(Some("0104"), "emit_machine_items_with_plan");

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
                } => match get_loc(*dst)? {
                    AllocLocation::Reg(r) => {
                        items.push(MachineItem::Inst(MachineInst::MovImm64 {
                            dst: r,
                            imm: *imm,
                            provenance: provenance.clone(),
                        }));
                    }
                    AllocLocation::SpillSlot(slot) => {
                        items.push(MachineItem::Inst(MachineInst::MovImm64 {
                            dst: SCRATCH_REG_A,
                            imm: *imm,
                            provenance: provenance.clone(),
                        }));
                        items.push(MachineItem::Inst(MachineInst::MovStore {
                            base: X86Reg::Rsp,
                            disp: (slot * 8) as i32,
                            src: SCRATCH_REG_A,
                            provenance: provenance.clone(),
                        }));
                    }
                },
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
                    let loc_rhs = get_loc(*rhs)?;

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

                    // Resolve physical register for rhs
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
                if let Some(vreg) = val {
                    match get_loc(*vreg)? {
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

                // Epilogue: restore stack pointer if frame was allocated
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
