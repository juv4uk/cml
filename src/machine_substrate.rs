//! Lisp-Authored Assembler Substrate for CML.
//!
//! Provides deterministic round-trip serialization and deserialization between
//! structured `MachineInst` / `MachineItem` target instructions and Lisp S-expressions.
//!
//! Epistemology & Authority:
//! - Target machine instructions are strictly target facts (registers, opcodes, labels).
//! - They carry NO source-language semantic IDs (semantic_id = None).
//! - They carry NO high-level surface spellings (e.g. `+`, `add`, `додати`).
//! - CML remains the low-level encoding and validation mechanism.
//! - Lisp macros / macro-atoms author reusable patterns (tag-fixnum, frame setup, ABI)
//!   without embedding those semantics into the Rust byte encoder.

use crate::ast::Expr;
use crate::machine_inst::{
    AluOp, CondCode, MachineInst, MachineItem, Provenance, X86Reg, assemble_program,
};
use crate::macros::MacroExpander;
use crate::parser::parse;
use std::fmt;

/// Errors during S-expression machine instruction parsing or validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineSubstrateError {
    ParseError(String),
    MalformedForm(String),
    UnknownTarget(String),
    UnknownOp(String),
    UnknownRegister(String),
    UnknownCondition(String),
    InvalidImmediate(String),
    AssemblyError(String),
    AuthorityViolation(String),
}

impl fmt::Display for MachineSubstrateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "machine-substrate parse error: {msg}"),
            Self::MalformedForm(msg) => write!(f, "machine-substrate malformed form: {msg}"),
            Self::UnknownTarget(tgt) => write!(f, "machine-substrate unknown target: {tgt}"),
            Self::UnknownOp(op) => write!(f, "machine-substrate unknown operation: {op}"),
            Self::UnknownRegister(reg) => write!(f, "machine-substrate unknown register: {reg}"),
            Self::UnknownCondition(c) => write!(f, "machine-substrate unknown condition: {c}"),
            Self::InvalidImmediate(msg) => write!(f, "machine-substrate invalid immediate: {msg}"),
            Self::AssemblyError(msg) => write!(f, "machine-substrate assembly error: {msg}"),
            Self::AuthorityViolation(msg) => {
                write!(f, "machine-substrate authority boundary violation: {msg}")
            }
        }
    }
}

impl std::error::Error for MachineSubstrateError {}

const SUBSTRATE_PROVENANCE: Provenance = Provenance::new(None, "lisp-authored assembler substrate");

/// Serialize a `MachineInst` to its canonical S-expression string.
pub fn inst_to_sexp(inst: &MachineInst) -> String {
    match inst {
        MachineInst::Rdtsc { .. } => "(x86 rdtsc)".to_string(),
        MachineInst::ShlImm { reg, imm, .. } => {
            format!("(x86 shl-imm {} {imm})", reg.raw_name())
        }
        MachineInst::ShrImm { reg, imm, .. } => {
            format!("(x86 shr-imm {} {imm})", reg.raw_name())
        }
        MachineInst::SarImm { reg, imm, .. } => {
            format!("(x86 sar-imm {} {imm})", reg.raw_name())
        }
        MachineInst::AluRegReg { op, dst, src, .. } => {
            format!(
                "(x86 alu-reg-reg {} {} {})",
                op.raw_name(),
                dst.raw_name(),
                src.raw_name()
            )
        }
        MachineInst::AluImm8 { op, dst, imm, .. } => {
            format!("(x86 alu-imm8 {} {} {imm})", op.raw_name(), dst.raw_name())
        }
        MachineInst::AluImm32 { op, dst, imm, .. } => {
            format!("(x86 alu-imm32 {} {} {imm})", op.raw_name(), dst.raw_name())
        }
        MachineInst::Lea {
            dst, base, disp, ..
        } => {
            format!("(x86 lea {} {} {disp})", dst.raw_name(), base.raw_name())
        }
        MachineInst::MovRegReg { dst, src, .. } => {
            format!("(x86 mov-reg-reg {} {})", dst.raw_name(), src.raw_name())
        }
        MachineInst::MovStore {
            base, disp, src, ..
        } => {
            format!(
                "(x86 mov-store {} {disp} {})",
                base.raw_name(),
                src.raw_name()
            )
        }
        MachineInst::MovLoad {
            dst, base, disp, ..
        } => {
            format!(
                "(x86 mov-load {} {} {disp})",
                dst.raw_name(),
                base.raw_name()
            )
        }
        MachineInst::MovImm64 { dst, imm, .. } => {
            format!("(x86 mov-imm64 {} {imm})", dst.raw_name())
        }
        MachineInst::TestRegReg { reg1, reg2, .. } => {
            format!("(x86 test-reg-reg {} {})", reg1.raw_name(), reg2.raw_name())
        }
        MachineInst::PushReg { reg, .. } => {
            format!("(x86 push-reg {})", reg.raw_name())
        }
        MachineInst::PopReg { reg, .. } => {
            format!("(x86 pop-reg {})", reg.raw_name())
        }
        MachineInst::Syscall { .. } => "(x86 syscall)".to_string(),
        MachineInst::JmpRel32 { disp, .. } => {
            format!("(x86 jmp-rel32 {disp})")
        }
        MachineInst::JccRel32 { cond, disp, .. } => {
            format!("(x86 jcc-rel32 {} {disp})", cond.name())
        }
        MachineInst::CallRel32 { disp, .. } => {
            format!("(x86 call-rel32 {disp})")
        }
        MachineInst::Nop { .. } => "(x86 nop)".to_string(),
        MachineInst::Ret { .. } => "(x86 ret)".to_string(),
    }
}

/// Serialize a `MachineItem` to its canonical S-expression string.
pub fn item_to_sexp(item: &MachineItem) -> String {
    match item {
        MachineItem::Label(name) => format!("(x86 label {name})"),
        MachineItem::Inst(inst) => inst_to_sexp(inst),
        MachineItem::JmpLabel { target, .. } => format!("(x86 jmp-label {target})"),
        MachineItem::JccLabel { cond, target, .. } => {
            format!("(x86 jcc-label {} {target})", cond.name())
        }
        MachineItem::CallLabel { target, .. } => format!("(x86 call-label {target})"),
    }
}

/// Serialize a sequence of `MachineItem`s to a newline-separated S-expression program.
pub fn program_to_sexp(items: &[MachineItem]) -> String {
    items
        .iter()
        .map(item_to_sexp)
        .collect::<Vec<_>>()
        .join("\n")
}

fn expect_symbol(expr: &Expr, context: &str) -> Result<String, MachineSubstrateError> {
    match expr {
        Expr::Symbol(s) => Ok(s.clone()),
        other => Err(MachineSubstrateError::MalformedForm(format!(
            "expected symbol in {context}, found {other:?}"
        ))),
    }
}

fn expect_integer(expr: &Expr, context: &str) -> Result<i64, MachineSubstrateError> {
    match expr {
        Expr::Integer(i) => Ok(*i),
        other => Err(MachineSubstrateError::MalformedForm(format!(
            "expected integer in {context}, found {other:?}"
        ))),
    }
}

fn parse_reg(expr: &Expr) -> Result<X86Reg, MachineSubstrateError> {
    let sym = expect_symbol(expr, "register operand")?;
    X86Reg::from_name(&sym).ok_or_else(|| MachineSubstrateError::UnknownRegister(sym))
}

fn parse_alu_op(expr: &Expr) -> Result<AluOp, MachineSubstrateError> {
    let sym = expect_symbol(expr, "alu operation")?;
    AluOp::from_name(&sym).ok_or_else(|| MachineSubstrateError::UnknownOp(sym))
}

fn parse_cond_code(expr: &Expr) -> Result<CondCode, MachineSubstrateError> {
    let sym = expect_symbol(expr, "condition code")?;
    CondCode::from_name(&sym).ok_or_else(|| MachineSubstrateError::UnknownCondition(sym))
}

/// Parse a single `MachineItem` from an `Expr`.
pub fn item_from_sexp(expr: &Expr) -> Result<MachineItem, MachineSubstrateError> {
    let Expr::List(list) = expr else {
        return Err(MachineSubstrateError::MalformedForm(format!(
            "expected list starting with target (x86 ...), found {expr:?}"
        )));
    };

    if list.is_empty() {
        return Err(MachineSubstrateError::MalformedForm(
            "empty form".to_string(),
        ));
    }

    let target = expect_symbol(&list[0], "target selector")?;
    if target != "x86" {
        return Err(MachineSubstrateError::UnknownTarget(target));
    }

    if list.len() < 2 {
        return Err(MachineSubstrateError::MalformedForm(
            "missing operation in (x86 ...)".to_string(),
        ));
    }

    let op_symbol = expect_symbol(&list[1], "machine item operator")?;

    match op_symbol.as_str() {
        "label" => {
            if list.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 label <name>)".to_string(),
                ));
            }
            let name = expect_symbol(&list[2], "label name")?;
            Ok(MachineItem::Label(name))
        }
        "jmp-label" => {
            if list.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 jmp-label <target>)".to_string(),
                ));
            }
            let target = expect_symbol(&list[2], "jmp target")?;
            Ok(MachineItem::JmpLabel {
                target,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "jcc-label" => {
            if list.len() != 4 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 jcc-label <cond> <target>)".to_string(),
                ));
            }
            let cond = parse_cond_code(&list[2])?;
            let target = expect_symbol(&list[3], "jcc target")?;
            Ok(MachineItem::JccLabel {
                cond,
                target,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "call-label" => {
            if list.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 call-label <target>)".to_string(),
                ));
            }
            let target = expect_symbol(&list[2], "call target")?;
            Ok(MachineItem::CallLabel {
                target,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        _ => {
            // It's an instruction
            let inst = inst_from_elements(&op_symbol, &list[2..])?;
            Ok(MachineItem::Inst(inst))
        }
    }
}

fn inst_from_elements(op: &str, args: &[Expr]) -> Result<MachineInst, MachineSubstrateError> {
    match op {
        "rdtsc" => {
            if !args.is_empty() {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 rdtsc)".to_string(),
                ));
            }
            Ok(MachineInst::Rdtsc {
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "shl-imm" => {
            if args.len() != 2 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 shl-imm <reg> <imm8>)".to_string(),
                ));
            }
            let reg = parse_reg(&args[0])?;
            let imm = expect_integer(&args[1], "shl immediate")? as u8;
            Ok(MachineInst::ShlImm {
                reg,
                imm,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "shr-imm" => {
            if args.len() != 2 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 shr-imm <reg> <imm8>)".to_string(),
                ));
            }
            let reg = parse_reg(&args[0])?;
            let imm = expect_integer(&args[1], "shr immediate")? as u8;
            Ok(MachineInst::ShrImm {
                reg,
                imm,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "sar-imm" => {
            if args.len() != 2 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 sar-imm <reg> <imm8>)".to_string(),
                ));
            }
            let reg = parse_reg(&args[0])?;
            let imm = expect_integer(&args[1], "sar immediate")? as u8;
            Ok(MachineInst::SarImm {
                reg,
                imm,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "alu-reg-reg" => {
            if args.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 alu-reg-reg <op> <dst> <src>)".to_string(),
                ));
            }
            let op = parse_alu_op(&args[0])?;
            let dst = parse_reg(&args[1])?;
            let src = parse_reg(&args[2])?;
            Ok(MachineInst::AluRegReg {
                op,
                dst,
                src,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "alu-imm8" => {
            if args.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 alu-imm8 <op> <dst> <imm8>)".to_string(),
                ));
            }
            let op = parse_alu_op(&args[0])?;
            let dst = parse_reg(&args[1])?;
            let imm = expect_integer(&args[2], "alu imm8")? as i8;
            Ok(MachineInst::AluImm8 {
                op,
                dst,
                imm,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "alu-imm32" => {
            if args.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 alu-imm32 <op> <dst> <imm32>)".to_string(),
                ));
            }
            let op = parse_alu_op(&args[0])?;
            let dst = parse_reg(&args[1])?;
            let imm = expect_integer(&args[2], "alu imm32")? as i32;
            Ok(MachineInst::AluImm32 {
                op,
                dst,
                imm,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "lea" => {
            if args.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 lea <dst> <base> <disp>)".to_string(),
                ));
            }
            let dst = parse_reg(&args[0])?;
            let base = parse_reg(&args[1])?;
            let disp = expect_integer(&args[2], "lea displacement")? as i32;
            Ok(MachineInst::Lea {
                dst,
                base,
                disp,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "mov-reg-reg" => {
            if args.len() != 2 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 mov-reg-reg <dst> <src>)".to_string(),
                ));
            }
            let dst = parse_reg(&args[0])?;
            let src = parse_reg(&args[1])?;
            Ok(MachineInst::MovRegReg {
                dst,
                src,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "mov-store" => {
            if args.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 mov-store <base> <disp> <src>)".to_string(),
                ));
            }
            let base = parse_reg(&args[0])?;
            let disp = expect_integer(&args[1], "store displacement")? as i32;
            let src = parse_reg(&args[2])?;
            Ok(MachineInst::MovStore {
                base,
                disp,
                src,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "mov-load" => {
            if args.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 mov-load <dst> <base> <disp>)".to_string(),
                ));
            }
            let dst = parse_reg(&args[0])?;
            let base = parse_reg(&args[1])?;
            let disp = expect_integer(&args[2], "load displacement")? as i32;
            Ok(MachineInst::MovLoad {
                dst,
                base,
                disp,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "mov-imm64" => {
            if args.len() != 2 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 mov-imm64 <dst> <imm64>)".to_string(),
                ));
            }
            let dst = parse_reg(&args[0])?;
            let imm = expect_integer(&args[1], "imm64")? as u64;
            Ok(MachineInst::MovImm64 {
                dst,
                imm,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "test-reg-reg" => {
            if args.len() != 2 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 test-reg-reg <reg1> <reg2>)".to_string(),
                ));
            }
            let reg1 = parse_reg(&args[0])?;
            let reg2 = parse_reg(&args[1])?;
            Ok(MachineInst::TestRegReg {
                reg1,
                reg2,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "push-reg" => {
            if args.len() != 1 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 push-reg <reg>)".to_string(),
                ));
            }
            let reg = parse_reg(&args[0])?;
            Ok(MachineInst::PushReg {
                reg,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "pop-reg" => {
            if args.len() != 1 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 pop-reg <reg>)".to_string(),
                ));
            }
            let reg = parse_reg(&args[0])?;
            Ok(MachineInst::PopReg {
                reg,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "syscall" => {
            if !args.is_empty() {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 syscall)".to_string(),
                ));
            }
            Ok(MachineInst::Syscall {
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "jmp-rel32" => {
            if args.len() != 1 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 jmp-rel32 <disp>)".to_string(),
                ));
            }
            let disp = expect_integer(&args[0], "jmp displacement")? as i32;
            Ok(MachineInst::JmpRel32 {
                disp,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "jcc-rel32" => {
            if args.len() != 2 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 jcc-rel32 <cond> <disp>)".to_string(),
                ));
            }
            let cond = parse_cond_code(&args[0])?;
            let disp = expect_integer(&args[1], "jcc displacement")? as i32;
            Ok(MachineInst::JccRel32 {
                cond,
                disp,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "call-rel32" => {
            if args.len() != 1 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 call-rel32 <disp>)".to_string(),
                ));
            }
            let disp = expect_integer(&args[0], "call displacement")? as i32;
            Ok(MachineInst::CallRel32 {
                disp,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "nop" => {
            if !args.is_empty() {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 nop)".to_string(),
                ));
            }
            Ok(MachineInst::Nop {
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "ret" => {
            if !args.is_empty() {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 ret)".to_string(),
                ));
            }
            Ok(MachineInst::Ret {
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        other => Err(MachineSubstrateError::UnknownOp(other.to_string())),
    }
}

/// Parse a multi-line S-expression program into a vector of `MachineItem`s.
pub fn parse_machine_program(source: &str) -> Result<Vec<MachineItem>, MachineSubstrateError> {
    let exprs = parse(source).map_err(|e| MachineSubstrateError::ParseError(e.to_string()))?;
    exprs.iter().map(item_from_sexp).collect()
}

/// Parse and assemble an S-expression program directly to physical x86-64 machine bytes.
pub fn assemble_sexp_program(source: &str) -> Result<Vec<u8>, MachineSubstrateError> {
    let items = parse_machine_program(source)?;
    assemble_program(&items).map_err(MachineSubstrateError::AssemblyError)
}

/// Macro-atom expander: expands Lisp macro definitions and invocations over machine instructions,
/// then parses the resulting expressions into `MachineItem`s.
pub fn expand_macro_atoms(source: &str) -> Result<Vec<MachineItem>, MachineSubstrateError> {
    let exprs = parse(source).map_err(|e| MachineSubstrateError::ParseError(e.to_string()))?;
    let mut expander = MacroExpander::new();
    let expanded = expander
        .process(&exprs)
        .map_err(|e| MachineSubstrateError::MalformedForm(e.to_string()))?;
    expanded.iter().map(item_from_sexp).collect()
}
