//! Backend-local x86 Lowered Intermediate Representation (LIR).
//!
//! # Architecture & Scope (ADR-004 & Issue #54)
//!
//! Under ADR-004, the shared semantic `Ir` (`src/ir.rs`) remains strictly backend-neutral
//! and contains no machine register hints, basic blocks, spills, or target instructions.
//!
//! This module introduces the backend-local x86 lowered layer featuring:
//! 1. **Virtual Registers (`VReg`)**: Three-address virtual register values decoupled from
//!    physical x86 registers.
//! 2. **Basic Blocks (`LirBlock`) & Control Flow Graph (`CFG`)**: Deterministic block
//!    identifiers (`BlockId`) with explicit terminators (`Jmp`, `BranchCond`, `Ret`).
//! 3. **Deterministic Lowering**: Lowers semantic `Ir` into `LirFunction`.
//! 4. **Inspectable Dump**: Clean human-readable text projection of blocks, vregs, and provenance.
//! 5. **Emission to `MachineItem`**: Deterministically materializes physical machine items.
//!
//! # Мовна межа та авторитет (Authority Boundary)
//!
//! Цей шар належить виключно бекенду компілятора CML як проміжне представлення для
//! оптимізацій (#55 Unboxing, #58 Inlining, #57 DCE, #56 RegAlloc). Він не змінює
//! семантику мови `my-lisp` і не додає жодних сторонніх варіантів у спільний `src/ir.rs`.

use crate::ir::{Ir, PrimOp};
use crate::machine_inst::{AluOp, CondCode, MachineItem, Provenance};
use std::collections::HashMap;
use std::fmt;

/// Virtual general-purpose register identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VReg(pub u32);

impl fmt::Display for VReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

/// Basic block identifier within a CFG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u32);

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

/// Relational condition code for conditional branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LirCond {
    Equal,
    NotEqual,
    LessThan,
    LessEqual,
    GreaterThan,
    GreaterEqual,
}

impl LirCond {
    pub const fn to_x86_cond(self) -> CondCode {
        match self {
            Self::Equal => CondCode::Equal,
            Self::NotEqual => CondCode::NotEqual,
            Self::LessThan => CondCode::Less,
            Self::LessEqual => CondCode::LessEqual,
            Self::GreaterThan => CondCode::Greater,
            Self::GreaterEqual => CondCode::GreaterEqual,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Equal => "eq",
            Self::NotEqual => "ne",
            Self::LessThan => "lt",
            Self::LessEqual => "le",
            Self::GreaterThan => "gt",
            Self::GreaterEqual => "ge",
        }
    }
}

/// Binary ALU operation in LIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LirAluOp {
    Add,
    Sub,
    And,
    Or,
    Xor,
}

impl LirAluOp {
    pub const fn to_x86_alu(self) -> AluOp {
        match self {
            Self::Add => AluOp::Add,
            Self::Sub => AluOp::Sub,
            Self::And => AluOp::And,
            Self::Or => AluOp::Or,
            Self::Xor => AluOp::Xor,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
        }
    }
}

/// A linear instruction inside a basic block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LirInst {
    /// Load 64-bit integer constant into virtual register: `dst = imm`
    Const64 {
        dst: VReg,
        imm: u64,
        provenance: Provenance,
    },
    /// Copy from source register to destination: `dst = src`
    Copy {
        dst: VReg,
        src: VReg,
        provenance: Provenance,
    },
    /// Three-address binary ALU operation: `dst = lhs OP rhs`
    Alu {
        op: LirAluOp,
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        provenance: Provenance,
    },
    /// Compare two virtual registers, setting condition flags: `cmp lhs, rhs`
    Cmp {
        lhs: VReg,
        rhs: VReg,
        provenance: Provenance,
    },
    /// Untag a boxed fixnum into raw 64-bit integer: `dst = src >> 3`
    UnboxFixnum {
        dst: VReg,
        src: VReg,
        provenance: Provenance,
    },
    /// Tag a raw 64-bit integer into a boxed fixnum: `dst = (src << 3) | Tag::Fixnum`
    BoxFixnum {
        dst: VReg,
        src: VReg,
        provenance: Provenance,
    },
}

impl LirInst {
    pub fn destination(&self) -> Option<VReg> {
        match self {
            Self::Const64 { dst, .. }
            | Self::Copy { dst, .. }
            | Self::Alu { dst, .. }
            | Self::UnboxFixnum { dst, .. }
            | Self::BoxFixnum { dst, .. } => Some(*dst),
            Self::Cmp { .. } => None,
        }
    }

    pub fn provenance(&self) -> &Provenance {
        match self {
            Self::Const64 { provenance, .. }
            | Self::Copy { provenance, .. }
            | Self::Alu { provenance, .. }
            | Self::Cmp { provenance, .. }
            | Self::UnboxFixnum { provenance, .. }
            | Self::BoxFixnum { provenance, .. } => provenance,
        }
    }
}

/// Explicit block terminator transferring control or returning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LirTerminator {
    /// Unconditional jump to a target block.
    Jmp {
        target: BlockId,
        provenance: Provenance,
    },
    /// Conditional branch based on condition flags.
    BranchCond {
        cond: LirCond,
        true_block: BlockId,
        false_block: BlockId,
        provenance: Provenance,
    },
    /// Function return.
    Ret {
        val: Option<VReg>,
        provenance: Provenance,
    },
}

impl LirTerminator {
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Self::Jmp { target, .. } => vec![*target],
            Self::BranchCond {
                true_block,
                false_block,
                ..
            } => vec![*true_block, *false_block],
            Self::Ret { .. } => Vec::new(),
        }
    }

    pub fn provenance(&self) -> &Provenance {
        match self {
            Self::Jmp { provenance, .. }
            | Self::BranchCond { provenance, .. }
            | Self::Ret { provenance, .. } => provenance,
        }
    }
}

/// Basic block containing straight-line instructions and an explicit terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirBlock {
    pub id: BlockId,
    pub instructions: Vec<LirInst>,
    pub terminator: LirTerminator,
}

/// A lowered function unit with its own CFG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirFunction {
    pub name: String,
    pub entry: BlockId,
    pub blocks: Vec<LirBlock>,
    next_vreg_id: u32,
    next_block_id: u32,
    pub provenance: Provenance,
}

impl LirFunction {
    pub fn new(name: impl Into<String>, provenance: Provenance) -> Self {
        let entry = BlockId(0);
        let initial_block = LirBlock {
            id: entry,
            instructions: Vec::new(),
            terminator: LirTerminator::Ret {
                val: None,
                provenance: provenance.clone(),
            },
        };

        Self {
            name: name.into(),
            entry,
            blocks: vec![initial_block],
            next_vreg_id: 0,
            next_block_id: 1,
            provenance,
        }
    }

    /// Allocates a fresh virtual register.
    pub fn alloc_vreg(&mut self) -> VReg {
        let vreg = VReg(self.next_vreg_id);
        self.next_vreg_id += 1;
        vreg
    }

    /// Allocates a new empty basic block with a dummy ret terminator.
    pub fn create_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block_id);
        self.next_block_id += 1;
        self.blocks.push(LirBlock {
            id,
            instructions: Vec::new(),
            terminator: LirTerminator::Ret {
                val: None,
                provenance: self.provenance.clone(),
            },
        });
        id
    }

    pub fn block(&self, id: BlockId) -> Option<&LirBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    pub fn block_mut(&mut self, id: BlockId) -> Option<&mut LirBlock> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    /// Returns predecessor block IDs for a given block.
    pub fn predecessors(&self, target: BlockId) -> Vec<BlockId> {
        let mut preds = Vec::new();
        for b in &self.blocks {
            if b.terminator.successors().contains(&target) {
                preds.push(b.id);
            }
        }
        preds
    }

    /// Generates a clean human-readable debug representation of the CFG.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "function {} (entry: {}) {{\n",
            self.name, self.entry
        ));

        for block in &self.blocks {
            out.push_str(&format!("  {}:\n", block.id));
            for inst in &block.instructions {
                out.push_str("    ");
                match inst {
                    LirInst::Const64 { dst, imm, .. } => {
                        out.push_str(&format!("{dst} = const64 {imm}\n"));
                    }
                    LirInst::Copy { dst, src, .. } => {
                        out.push_str(&format!("{dst} = copy {src}\n"));
                    }
                    LirInst::Alu {
                        op, dst, lhs, rhs, ..
                    } => {
                        out.push_str(&format!("{dst} = {} {lhs}, {rhs}\n", op.name()));
                    }
                    LirInst::Cmp { lhs, rhs, .. } => {
                        out.push_str(&format!("cmp {lhs}, {rhs}\n"));
                    }
                    LirInst::UnboxFixnum { dst, src, .. } => {
                        out.push_str(&format!("{dst} = unbox_fixnum {src}\n"));
                    }
                    LirInst::BoxFixnum { dst, src, .. } => {
                        out.push_str(&format!("{dst} = box_fixnum {src}\n"));
                    }
                }
            }

            out.push_str("    ");
            match &block.terminator {
                LirTerminator::Jmp { target, .. } => {
                    out.push_str(&format!("jmp {target}\n"));
                }
                LirTerminator::BranchCond {
                    cond,
                    true_block,
                    false_block,
                    ..
                } => {
                    out.push_str(&format!(
                        "branch_{} {}, {}\n",
                        cond.name(),
                        true_block,
                        false_block
                    ));
                }
                LirTerminator::Ret { val, .. } => {
                    if let Some(v) = val {
                        out.push_str(&format!("ret {v}\n"));
                    } else {
                        out.push_str("ret\n");
                    }
                }
            }
        }

        out.push_str("}\n");
        out
    }
}

/// Lowering error for semantic `Ir` -> `LirFunction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LirLowerError {
    Unsupported(String),
    InvalidArity { expected: usize, found: usize },
    EmptyProgram,
}

impl fmt::Display for LirLowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(msg) => write!(f, "unsupported semantic IR form for x86 LIR: {msg}"),
            Self::InvalidArity { expected, found } => {
                write!(
                    f,
                    "invalid arity in semantic IR: expected {expected}, found {found}"
                )
            }
            Self::EmptyProgram => write!(f, "cannot lower an empty program to x86 LIR"),
        }
    }
}

impl std::error::Error for LirLowerError {}

/// Context tracking current block and variable bindings during lowering.
struct LowerContext<'a> {
    func: &'a mut LirFunction,
    current_block: BlockId,
    bindings: HashMap<String, VReg>,
}

impl<'a> LowerContext<'a> {
    fn new(func: &'a mut LirFunction, entry: BlockId) -> Self {
        Self {
            func,
            current_block: entry,
            bindings: HashMap::new(),
        }
    }

    fn emit(&mut self, inst: LirInst) {
        if let Some(block) = self.func.block_mut(self.current_block) {
            block.instructions.push(inst);
        }
    }

    fn set_terminator(&mut self, term: LirTerminator) {
        if let Some(block) = self.func.block_mut(self.current_block) {
            block.terminator = term;
        }
    }
}

fn lower_expr(expr: &Ir, ctx: &mut LowerContext) -> Result<VReg, LirLowerError> {
    let prov = Provenance::new(None, "lower_ir_to_lir");
    match expr {
        Ir::Int(val) => {
            let v = ctx.func.alloc_vreg();
            ctx.emit(LirInst::Const64 {
                dst: v,
                imm: *val as u64,
                provenance: prov,
            });
            Ok(v)
        }
        Ir::Var(name) => {
            if let Some(&vreg) = ctx.bindings.get(name) {
                Ok(vreg)
            } else {
                Err(LirLowerError::Unsupported(format!(
                    "unbound variable: {name}"
                )))
            }
        }
        Ir::Prim { op, args } => match op {
            PrimOp::Add | PrimOp::Sub => {
                if args.len() != 2 {
                    return Err(LirLowerError::InvalidArity {
                        expected: 2,
                        found: args.len(),
                    });
                }
                let lhs = lower_expr(&args[0], ctx)?;
                let rhs = lower_expr(&args[1], ctx)?;
                let dst = ctx.func.alloc_vreg();
                let lir_op = match op {
                    PrimOp::Add => LirAluOp::Add,
                    PrimOp::Sub => LirAluOp::Sub,
                    _ => unreachable!(),
                };
                ctx.emit(LirInst::Alu {
                    op: lir_op,
                    dst,
                    lhs,
                    rhs,
                    provenance: prov,
                });
                Ok(dst)
            }
            _ => Err(LirLowerError::Unsupported(format!(
                "primitive op {:?} is not admitted in scalar LIR slice",
                op
            ))),
        },
        Ir::Cond { branches } => {
            if branches.is_empty() {
                return Err(LirLowerError::Unsupported(
                    "empty cond branches".to_string(),
                ));
            }

            let join_block = ctx.func.create_block();
            let result_vreg = ctx.func.alloc_vreg();

            for (idx, (test_expr, body_expr)) in branches.iter().enumerate() {
                let is_last = idx + 1 == branches.len();

                // If test is literal True or 't', unconditionally execute body
                if *test_expr == Ir::True {
                    let body_val = lower_expr(body_expr, ctx)?;
                    ctx.emit(LirInst::Copy {
                        dst: result_vreg,
                        src: body_val,
                        provenance: prov.clone(),
                    });
                    ctx.set_terminator(LirTerminator::Jmp {
                        target: join_block,
                        provenance: prov.clone(),
                    });
                    break;
                }

                // If test is (eq lhs rhs)
                if let Ir::Prim {
                    op: PrimOp::Eq,
                    args,
                } = test_expr
                {
                    if args.len() != 2 {
                        return Err(LirLowerError::InvalidArity {
                            expected: 2,
                            found: args.len(),
                        });
                    }
                    let lhs = lower_expr(&args[0], ctx)?;
                    let rhs = lower_expr(&args[1], ctx)?;
                    ctx.emit(LirInst::Cmp {
                        lhs,
                        rhs,
                        provenance: prov.clone(),
                    });

                    let then_block = ctx.func.create_block();
                    let next_test_block = if is_last {
                        join_block
                    } else {
                        ctx.func.create_block()
                    };

                    ctx.set_terminator(LirTerminator::BranchCond {
                        cond: LirCond::Equal,
                        true_block: then_block,
                        false_block: next_test_block,
                        provenance: prov.clone(),
                    });

                    // Lower then branch
                    ctx.current_block = then_block;
                    let body_val = lower_expr(body_expr, ctx)?;
                    ctx.emit(LirInst::Copy {
                        dst: result_vreg,
                        src: body_val,
                        provenance: prov.clone(),
                    });
                    ctx.set_terminator(LirTerminator::Jmp {
                        target: join_block,
                        provenance: prov.clone(),
                    });

                    // Switch to next test block
                    ctx.current_block = next_test_block;
                } else {
                    return Err(LirLowerError::Unsupported(format!(
                        "cond test form {:?} is not admitted in scalar LIR slice (only (eq a b) or t)",
                        test_expr
                    )));
                }
            }

            ctx.current_block = join_block;
            Ok(result_vreg)
        }
        Ir::Let { bindings, body } => {
            let mut bound_vregs = Vec::new();
            for (name, val_expr) in bindings {
                if let Ir::Lambda { .. } = val_expr {
                    continue;
                }
                let v = lower_expr(val_expr, ctx)?;
                bound_vregs.push((name.clone(), v));
            }
            for (name, v) in bound_vregs {
                ctx.bindings.insert(name, v);
            }
            lower_expr(body, ctx)
        }
        other => Err(LirLowerError::Unsupported(format!(
            "semantic IR form {:?} is outside the #54 scalar LIR slice",
            other
        ))),
    }
}

/// Lowers a semantic `Ir` program into an x86 `LirFunction` CFG.
pub fn lower_ir_to_lir(ir: &Ir) -> Result<LirFunction, LirLowerError> {
    let prov = Provenance::new(None, "lower_ir_to_lir");
    let mut func = LirFunction::new("main", prov.clone());
    let entry = func.entry;
    let mut ctx = LowerContext::new(&mut func, entry);

    let return_vreg = lower_expr(ir, &mut ctx)?;
    ctx.set_terminator(LirTerminator::Ret {
        val: Some(return_vreg),
        provenance: prov,
    });

    Ok(func)
}

/// Emission error when converting `LirFunction` to physical `MachineItem`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LirEmitError {
    RegisterExhaustion(String),
}

impl fmt::Display for LirEmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegisterExhaustion(msg) => write!(f, "register exhaustion: {msg}"),
        }
    }
}

impl std::error::Error for LirEmitError {}

/// Compiles an `LirFunction` CFG into a structured sequence of `MachineItem`s via register allocation.
pub fn lir_to_machine_items(func: &LirFunction) -> Result<Vec<MachineItem>, LirEmitError> {
    let plan = crate::x86_regalloc::allocate_registers(func)
        .map_err(|e| LirEmitError::RegisterExhaustion(format!("{e}")))?;
    crate::x86_regalloc::emit_machine_items_with_plan(func, &plan)
        .map_err(|e| LirEmitError::RegisterExhaustion(format!("{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lir_cfg_construction_and_dump() {
        let prov = Provenance::new(None, "test");
        let mut func = LirFunction::new("test_fn", prov.clone());
        let v0 = func.alloc_vreg();
        let v1 = func.alloc_vreg();
        let v2 = func.alloc_vreg();

        let entry = func.entry;
        func.block_mut(entry)
            .unwrap()
            .instructions
            .push(LirInst::Const64 {
                dst: v0,
                imm: 10,
                provenance: prov.clone(),
            });
        func.block_mut(entry)
            .unwrap()
            .instructions
            .push(LirInst::Const64 {
                dst: v1,
                imm: 32,
                provenance: prov.clone(),
            });
        func.block_mut(entry)
            .unwrap()
            .instructions
            .push(LirInst::Alu {
                op: LirAluOp::Add,
                dst: v2,
                lhs: v0,
                rhs: v1,
                provenance: prov.clone(),
            });
        func.block_mut(entry).unwrap().terminator = LirTerminator::Ret {
            val: Some(v2),
            provenance: prov,
        };

        let dump = func.dump();
        assert!(dump.contains("function test_fn (entry: bb0)"));
        assert!(dump.contains("v0 = const64 10"));
        assert!(dump.contains("v1 = const64 32"));
        assert!(dump.contains("v2 = add v0, v1"));
        assert!(dump.contains("ret v2"));
    }

    #[test]
    fn test_lower_arithmetic_witness() {
        let ir = Ir::Prim {
            op: PrimOp::Add,
            args: vec![Ir::Int(10), Ir::Int(32)],
        };
        let func = lower_ir_to_lir(&ir).expect("lower arithmetic witness");
        assert_eq!(func.blocks.len(), 1);
        let items = lir_to_machine_items(&func).expect("emit machine items");
        let bytes = crate::machine_inst::assemble_program(&items).expect("assemble");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_lower_cond_branch_witness() {
        let ir = Ir::Cond {
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
        let func = lower_ir_to_lir(&ir).expect("lower cond witness");
        assert!(
            func.blocks.len() >= 3,
            "CFG must contain multiple basic blocks"
        );
        let dump = func.dump();
        assert!(dump.contains("branch_eq"));
        assert!(dump.contains("ret"));
    }
}
