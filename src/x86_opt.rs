//! Local optimization pipeline for x86 Lowered IR (#57).
//!
//! # Architecture & Scope (ADR-004 & Issue #57)
//!
//! This module implements an inspectable, deterministic, semantics-preserving optimization
//! pipeline operating exclusively over backend-local `LirFunction` representations.
//!
//! ## Authority & Purity Invariants
//! - `my-lisp` owns language semantics, numeric types, and oracle evaluation.
//! - Backend-neutral `Ir` (`src/ir.rs`) remains strictly untouched (no optimizer hints or tables).
//! - All optimization passes live exclusively in this backend-local module.
//!
//! ## Optimization Passes
//! 1. **Constant Propagation & Folding**: Evaluates constant expressions when exact integer
//!    bounds and absence of overflow are statically proven.
//! 2. **Algebraic Identities**: Rewrites domain-valid identities (e.g. `x + 0 -> x`, `x - 0 -> x`).
//! 3. **Copy Propagation**: Replaces redundant register copies with original sources.
//! 4. **Dead Code Elimination (DCE)**: Eliminates unobservable virtual-register definitions.
//! 5. **Branch Simplification**: Replaces conditional branches with unconditional jumps when
//!    the branch condition is proven constant.
//! 6. **Unreachable Block Elimination**: Removes CFG blocks that cannot be reached from entry.

use crate::numeric_specialization::MAX_FIXNUM;
use crate::x86_lir::{LirAluOp, LirCond, LirFunction, LirInst, LirTerminator, VReg};
use std::collections::{HashMap, HashSet, VecDeque};

/// Configuration flags for enabling/disabling individual optimization passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalOptConfig {
    pub const_propagation: bool,
    pub const_folding: bool,
    pub copy_propagation: bool,
    pub dead_code_elimination: bool,
    pub unreachable_block_elimination: bool,
    pub branch_simplification: bool,
    pub algebraic_identities: bool,
}

impl LocalOptConfig {
    /// All optimizations disabled (baseline lowering).
    pub const fn all_disabled() -> Self {
        Self {
            const_propagation: false,
            const_folding: false,
            copy_propagation: false,
            dead_code_elimination: false,
            unreachable_block_elimination: false,
            branch_simplification: false,
            algebraic_identities: false,
        }
    }

    /// All local optimization passes enabled.
    pub const fn all_enabled() -> Self {
        Self {
            const_propagation: true,
            const_folding: true,
            copy_propagation: true,
            dead_code_elimination: true,
            unreachable_block_elimination: true,
            branch_simplification: true,
            algebraic_identities: true,
        }
    }
}

/// Statistics and accounting of transformations applied by the optimization pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OptReport {
    pub consts_folded: usize,
    pub consts_propagated: usize,
    pub copies_propagated: usize,
    pub dce_removed: usize,
    pub branches_simplified: usize,
    pub blocks_eliminated: usize,
    pub identities_applied: usize,
}

impl OptReport {
    pub fn total_transforms(&self) -> usize {
        self.consts_folded
            + self.consts_propagated
            + self.copies_propagated
            + self.dce_removed
            + self.branches_simplified
            + self.blocks_eliminated
            + self.identities_applied
    }

    pub fn dump(&self) -> String {
        format!(
            "OptReport: total={}, folded={}, const_prop={}, copy_prop={}, dce={}, branch_simp={}, unreachable_blocks={}, identities={}",
            self.total_transforms(),
            self.consts_folded,
            self.consts_propagated,
            self.copies_propagated,
            self.dce_removed,
            self.branches_simplified,
            self.blocks_eliminated,
            self.identities_applied
        )
    }
}

/// Runs constant propagation, constant folding, and algebraic identities within basic blocks.
fn optimize_block_instructions(
    func: &mut LirFunction,
    cfg: &LocalOptConfig,
    report: &mut OptReport,
) -> bool {
    let mut changed = false;

    // Track known constants and copy mappings across the function
    let mut const_map: HashMap<VReg, u64> = HashMap::new();
    let mut copy_map: HashMap<VReg, VReg> = HashMap::new();

    // First pass: collect constant definitions and copies
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                LirInst::Const64 { dst, imm, .. } => {
                    const_map.insert(*dst, *imm);
                }
                LirInst::Copy { dst, src, .. } => {
                    copy_map.insert(*dst, *src);
                }
                _ => {}
            }
        }
    }

    // Second pass: rewrite instructions
    for block in &mut func.blocks {
        let mut new_insts = Vec::with_capacity(block.instructions.len());

        for mut inst in block.instructions.drain(..) {
            // Copy propagation on inputs
            if cfg.copy_propagation {
                match &mut inst {
                    LirInst::Copy { src, .. } => {
                        if let Some(&orig) = copy_map.get(src) {
                            if orig != *src {
                                *src = orig;
                                report.copies_propagated += 1;
                                changed = true;
                            }
                        }
                    }
                    LirInst::Alu { lhs, rhs, .. } | LirInst::Cmp { lhs, rhs, .. } => {
                        if let Some(&orig) = copy_map.get(lhs) {
                            if orig != *lhs {
                                *lhs = orig;
                                report.copies_propagated += 1;
                                changed = true;
                            }
                        }
                        if let Some(&orig) = copy_map.get(rhs) {
                            if orig != *rhs {
                                *rhs = orig;
                                report.copies_propagated += 1;
                                changed = true;
                            }
                        }
                    }
                    LirInst::UnboxFixnum { src, .. } | LirInst::BoxFixnum { src, .. } => {
                        if let Some(&orig) = copy_map.get(src) {
                            if orig != *src {
                                *src = orig;
                                report.copies_propagated += 1;
                                changed = true;
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Algebraic identities
            if cfg.algebraic_identities {
                if let LirInst::Alu {
                    op,
                    dst,
                    lhs,
                    rhs,
                    provenance,
                } = &inst
                {
                    match op {
                        LirAluOp::Add => {
                            // x + 0 -> x
                            if const_map.get(rhs) == Some(&0) {
                                report.identities_applied += 1;
                                changed = true;
                                new_insts.push(LirInst::Copy {
                                    dst: *dst,
                                    src: *lhs,
                                    provenance: provenance.clone(),
                                });
                                continue;
                            }
                            // 0 + x -> x
                            if const_map.get(lhs) == Some(&0) {
                                report.identities_applied += 1;
                                changed = true;
                                new_insts.push(LirInst::Copy {
                                    dst: *dst,
                                    src: *rhs,
                                    provenance: provenance.clone(),
                                });
                                continue;
                            }
                        }
                        LirAluOp::Sub => {
                            // x - 0 -> x
                            if const_map.get(rhs) == Some(&0) {
                                report.identities_applied += 1;
                                changed = true;
                                new_insts.push(LirInst::Copy {
                                    dst: *dst,
                                    src: *lhs,
                                    provenance: provenance.clone(),
                                });
                                continue;
                            }
                        }
                        LirAluOp::Or => {
                            // x | 0 -> x
                            if const_map.get(rhs) == Some(&0) {
                                report.identities_applied += 1;
                                changed = true;
                                new_insts.push(LirInst::Copy {
                                    dst: *dst,
                                    src: *lhs,
                                    provenance: provenance.clone(),
                                });
                                continue;
                            }
                        }
                        LirAluOp::Xor => {
                            // x ^ 0 -> x
                            if const_map.get(rhs) == Some(&0) {
                                report.identities_applied += 1;
                                changed = true;
                                new_insts.push(LirInst::Copy {
                                    dst: *dst,
                                    src: *lhs,
                                    provenance: provenance.clone(),
                                });
                                continue;
                            }
                        }
                        LirAluOp::And => {
                            // x & x -> x
                            if lhs == rhs {
                                report.identities_applied += 1;
                                changed = true;
                                new_insts.push(LirInst::Copy {
                                    dst: *dst,
                                    src: *lhs,
                                    provenance: provenance.clone(),
                                });
                                continue;
                            }
                        }
                    }
                }
            }

            // Constant folding
            if cfg.const_folding {
                if let LirInst::Alu {
                    op,
                    dst,
                    lhs,
                    rhs,
                    provenance,
                } = &inst
                {
                    if let (Some(&c1), Some(&c2)) = (const_map.get(lhs), const_map.get(rhs)) {
                        // Safety rule: fold only when overflow does not occur and fits admitted domain
                        let folded = match op {
                            LirAluOp::Add => {
                                let (sum, o) = c1.overflowing_add(c2);
                                if !o && sum <= (MAX_FIXNUM as u64) {
                                    Some(sum)
                                } else {
                                    None
                                }
                            }
                            LirAluOp::Sub => {
                                if c1 >= c2 {
                                    Some(c1 - c2)
                                } else {
                                    None
                                }
                            }
                            LirAluOp::And => Some(c1 & c2),
                            LirAluOp::Or => Some(c1 | c2),
                            LirAluOp::Xor => Some(c1 ^ c2),
                        };

                        if let Some(val) = folded {
                            report.consts_folded += 1;
                            const_map.insert(*dst, val);
                            changed = true;
                            new_insts.push(LirInst::Const64 {
                                dst: *dst,
                                imm: val,
                                provenance: provenance.clone(),
                            });
                            continue;
                        }
                    }
                }
            }

            new_insts.push(inst);
        }

        block.instructions = new_insts;
    }

    changed
}

/// Simplifies conditional branches when the comparison operands are known constants.
fn simplify_branches(func: &mut LirFunction, cfg: &LocalOptConfig, report: &mut OptReport) -> bool {
    if !cfg.branch_simplification {
        return false;
    }

    let mut changed = false;

    // Collect constant map
    let mut const_map: HashMap<VReg, u64> = HashMap::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let LirInst::Const64 { dst, imm, .. } = inst {
                const_map.insert(*dst, *imm);
            }
        }
    }

    for block in &mut func.blocks {
        // Find if last instruction was a Cmp with known constants
        let mut last_cmp = None;
        if let Some(LirInst::Cmp { lhs, rhs, .. }) = block.instructions.last() {
            if let (Some(&c1), Some(&c2)) = (const_map.get(lhs), const_map.get(rhs)) {
                last_cmp = Some((c1, c2));
            }
        }

        if let Some((c1, c2)) = last_cmp {
            let mut new_term = None;
            if let LirTerminator::BranchCond {
                cond,
                true_block,
                false_block,
                provenance,
            } = &block.terminator
            {
                let is_true = match cond {
                    LirCond::Equal => c1 == c2,
                    LirCond::NotEqual => c1 != c2,
                    LirCond::LessThan => c1 < c2,
                    LirCond::LessEqual => c1 <= c2,
                    LirCond::GreaterThan => c1 > c2,
                    LirCond::GreaterEqual => c1 >= c2,
                };

                let target = if is_true { *true_block } else { *false_block };
                new_term = Some(LirTerminator::Jmp {
                    target,
                    provenance: provenance.clone(),
                });
            }

            if let Some(term) = new_term {
                block.terminator = term;
                // Pop the redundant Cmp instruction
                block.instructions.pop();
                report.branches_simplified += 1;
                changed = true;
            }
        }
    }

    changed
}

/// Eliminates unreachable basic blocks from the CFG.
fn eliminate_unreachable_blocks(
    func: &mut LirFunction,
    cfg: &LocalOptConfig,
    report: &mut OptReport,
) -> bool {
    if !cfg.unreachable_block_elimination {
        return false;
    }

    // Traverse CFG reachable from entry
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();

    reachable.insert(func.entry);
    queue.push_back(func.entry);

    while let Some(bid) = queue.pop_front() {
        if let Some(block) = func.block(bid) {
            for succ in block.terminator.successors() {
                if reachable.insert(succ) {
                    queue.push_back(succ);
                }
            }
        }
    }

    let initial_count = func.blocks.len();
    func.blocks.retain(|b| reachable.contains(&b.id));
    let removed = initial_count - func.blocks.len();

    if removed > 0 {
        report.blocks_eliminated += removed;
        true
    } else {
        false
    }
}

/// Eliminates dead virtual-register definitions whose values are never read.
fn eliminate_dead_code(
    func: &mut LirFunction,
    cfg: &LocalOptConfig,
    report: &mut OptReport,
) -> bool {
    if !cfg.dead_code_elimination {
        return false;
    }

    // Collect all used VRegs across instructions and terminators
    let mut used_vregs = HashSet::new();

    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                LirInst::Copy { src, .. }
                | LirInst::UnboxFixnum { src, .. }
                | LirInst::BoxFixnum { src, .. } => {
                    used_vregs.insert(*src);
                }
                LirInst::Alu { lhs, rhs, .. } | LirInst::Cmp { lhs, rhs, .. } => {
                    used_vregs.insert(*lhs);
                    used_vregs.insert(*rhs);
                }
                LirInst::Const64 { .. } => {}
            }
        }

        if let LirTerminator::Ret { val: Some(v), .. } = &block.terminator {
            used_vregs.insert(*v);
        }
    }

    let mut changed = false;

    for block in &mut func.blocks {
        let mut new_insts = Vec::with_capacity(block.instructions.len());

        for inst in block.instructions.drain(..) {
            // Check if instruction destination is unused and instruction is side-effect-free
            let is_dead = match &inst {
                LirInst::Const64 { dst, .. }
                | LirInst::Copy { dst, .. }
                | LirInst::Alu { dst, .. }
                | LirInst::UnboxFixnum { dst, .. }
                | LirInst::BoxFixnum { dst, .. } => !used_vregs.contains(dst),
                LirInst::Cmp { .. } => false, // Cmp affects flags, not dead
            };

            if is_dead {
                report.dce_removed += 1;
                changed = true;
            } else {
                new_insts.push(inst);
            }
        }

        block.instructions = new_insts;
    }

    changed
}

/// Optimizes an `LirFunction` using the configured local optimization passes.
pub fn optimize_lir(func: &mut LirFunction, config: LocalOptConfig) -> OptReport {
    let mut report = OptReport::default();

    // Iterate passes until fixed point or max 10 iterations
    for _ in 0..10 {
        let mut changed = false;
        changed |= optimize_block_instructions(func, &config, &mut report);
        changed |= simplify_branches(func, &config, &mut report);
        changed |= eliminate_unreachable_blocks(func, &config, &mut report);
        changed |= eliminate_dead_code(func, &config, &mut report);

        if !changed {
            break;
        }
    }

    report
}
