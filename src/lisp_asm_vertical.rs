//! Minimal Lisp-oriented lowering layer for the first native x86-64 vertical slice.
//!
//! Language meaning is already admitted in `Ir`; this module only selects a
//! small target program from that IR. It deliberately owns no source spelling
//! and allocates no semantic IDs. The resulting target executable contains no
//! C runtime and no Rust runtime.

use crate::ir::{Ir, PrimOp};
use crate::machine_inst::{AluOp, MachineInst, MachineItem, Provenance, X86Reg};

/// Errors from the deliberately bounded first Lisp -> assembler target slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerticalSliceError {
    EmptyProgram,
    UnsupportedIrVariant(&'static str),
    InvalidArity { expected: usize, actual: usize },
    FixnumOutOfRange(i64),
}

impl std::fmt::Display for VerticalSliceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyProgram => write!(f, "empty program has no execution entry"),
            Self::UnsupportedIrVariant(v) => {
                write!(f, "unsupported IR variant in vertical slice: {v}")
            }
            Self::InvalidArity { expected, actual } => {
                write!(f, "invalid arity: expected {expected}, got {actual}")
            }
            Self::FixnumOutOfRange(n) => write!(f, "fixnum out of range: {n}"),
        }
    }
}

impl std::error::Error for VerticalSliceError {}

/// Select executable x86-64 machine items for the admitted first slice.
///
/// Scope is intentionally small and fail-closed: one top-level fixnum or one
/// binary Add/Sub over literal fixnums. This is enough to prove the physical
/// Lisp-source -> CML -> x86-64 path without pretending the whole language is
/// already self-hosted.
pub fn select_arithmetic_slice(program: &[Ir]) -> Result<Vec<MachineItem>, VerticalSliceError> {
    if program.is_empty() {
        return Err(VerticalSliceError::EmptyProgram);
    }
    if program.len() != 1 {
        return Err(VerticalSliceError::UnsupportedIrVariant(
            "expected exactly one top-level expression",
        ));
    }

    let prov = Provenance::new(None, "Lisp-to-x86 vertical slice");
    let mut items = vec![MachineItem::Label("_start".to_string())];

    // Keep the target stack aligned before using it as the eight-byte result buffer.
    items.push(MachineItem::Inst(MachineInst::AluImm8 {
        op: AluOp::And,
        dst: X86Reg::Rsp,
        imm: -16,
        provenance: prov.clone(),
    }));

    match &program[0] {
        Ir::Int(value) => {
            let tagged = wsm_os_target::encode_fixnum(*value)
                .ok_or(VerticalSliceError::FixnumOutOfRange(*value))?;
            items.push(MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rax,
                imm: tagged,
                provenance: prov.clone(),
            }));
        }
        Ir::Prim { op, args } if matches!(op, PrimOp::Add | PrimOp::Sub) => {
            if args.len() != 2 {
                return Err(VerticalSliceError::InvalidArity {
                    expected: 2,
                    actual: args.len(),
                });
            }

            let literal = |ir: &Ir| match ir {
                Ir::Int(n) => Ok(*n),
                _ => Err(VerticalSliceError::UnsupportedIrVariant(
                    "non-integer arithmetic operand",
                )),
            };
            let a = literal(&args[0])?;
            let b = literal(&args[1])?;
            let tagged_a =
                wsm_os_target::encode_fixnum(a).ok_or(VerticalSliceError::FixnumOutOfRange(a))?;
            let tagged_b =
                wsm_os_target::encode_fixnum(b).ok_or(VerticalSliceError::FixnumOutOfRange(b))?;

            // Load canonical target words, untag, perform arithmetic, then retag.
            items.push(MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rcx,
                imm: tagged_a,
                provenance: prov.clone(),
            }));
            items.push(MachineItem::Inst(MachineInst::SarImm {
                reg: X86Reg::Rcx,
                imm: 3,
                provenance: prov.clone(),
            }));
            items.push(MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rdx,
                imm: tagged_b,
                provenance: prov.clone(),
            }));
            items.push(MachineItem::Inst(MachineInst::SarImm {
                reg: X86Reg::Rdx,
                imm: 3,
                provenance: prov.clone(),
            }));
            items.push(MachineItem::Inst(MachineInst::AluRegReg {
                op: match op {
                    PrimOp::Add => AluOp::Add,
                    PrimOp::Sub => AluOp::Sub,
                    _ => unreachable!(),
                },
                dst: X86Reg::Rcx,
                src: X86Reg::Rdx,
                provenance: prov.clone(),
            }));
            items.push(MachineItem::Inst(MachineInst::ShlImm {
                reg: X86Reg::Rcx,
                imm: 3,
                provenance: prov.clone(),
            }));
            items.push(MachineItem::Inst(MachineInst::AluImm8 {
                op: AluOp::Or,
                dst: X86Reg::Rcx,
                imm: wsm_os_target::Tag::Fixnum as i8,
                provenance: prov.clone(),
            }));
            items.push(MachineItem::Inst(MachineInst::MovRegReg {
                dst: X86Reg::Rax,
                src: X86Reg::Rcx,
                provenance: prov.clone(),
            }));
        }
        _ => {
            return Err(VerticalSliceError::UnsupportedIrVariant(
                "expected fixnum or binary arithmetic",
            ));
        }
    }

    // Preserve the tagged Lisp value as the observable result: write its exact
    // eight-byte target word to stdout, then untag only for the process exit code.
    items.push(MachineItem::Inst(MachineInst::PushReg {
        reg: X86Reg::Rax,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::MovRegReg {
        dst: X86Reg::Rsi,
        src: X86Reg::Rsp,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::MovImm64 {
        dst: X86Reg::Rdi,
        imm: 1,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::MovImm64 {
        dst: X86Reg::Rdx,
        imm: 8,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::MovImm64 {
        dst: X86Reg::Rax,
        imm: 1,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::Syscall {
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::PopReg {
        reg: X86Reg::Rax,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::MovRegReg {
        dst: X86Reg::Rdi,
        src: X86Reg::Rax,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::SarImm {
        reg: X86Reg::Rdi,
        imm: 3,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::MovImm64 {
        dst: X86Reg::Rax,
        imm: 60,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::Syscall { provenance: prov }));

    Ok(items)
}

/// Render the same structured target program as GNU assembler text.
///
/// This is a projection/oracle only. The direct-byte path does not depend on
/// GNU `as` or `ld`.
pub fn items_to_gnu_asm(items: &[MachineItem]) -> String {
    let mut text = String::from(".text\n.globl _start\n.type _start, @function\n");
    for item in items {
        match item {
            MachineItem::Label(label) => text.push_str(&format!("{label}:\n")),
            MachineItem::Inst(inst) => {
                text.push_str("    ");
                text.push_str(&inst.print_gnu_asm());
                text.push('\n');
            }
            MachineItem::JmpLabel { target, .. } => {
                text.push_str(&format!("    jmp {target}\n"));
            }
            MachineItem::JccLabel { cond, target, .. } => {
                text.push_str(&format!("    j{} {target}\n", cond.mnemonic_suffix()));
            }
            MachineItem::CallLabel { target, .. } => {
                text.push_str(&format!("    call {target}\n"));
            }
        }
    }
    text
}
