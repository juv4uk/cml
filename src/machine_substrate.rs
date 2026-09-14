//! Lisp-authored assembler substrate for CML.
//!
//! This layer maps target-specific Lisp S-expressions to structured
//! `MachineItem`s and back. It owns machine mechanism only: no source-language
//! semantic IDs are allocated here and no EN/UK/UKR/SA surface spellings are
//! interpreted here.

use crate::ast::Expr;
use crate::machine_inst::{
    AluOp, CondCode, MachineInst, MachineItem, Provenance, X86Reg, assemble_program,
};
use crate::macros::MacroExpander;
use crate::parser::parse;
use std::fmt;

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
}

impl fmt::Display for MachineSubstrateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "machine-substrate parse error: {msg}"),
            Self::MalformedForm(msg) => write!(f, "machine-substrate malformed form: {msg}"),
            Self::UnknownTarget(target) => write!(f, "machine-substrate unknown target: {target}"),
            Self::UnknownOp(op) => write!(f, "machine-substrate unknown operation: {op}"),
            Self::UnknownRegister(reg) => write!(f, "machine-substrate unknown register: {reg}"),
            Self::UnknownCondition(cond) => {
                write!(f, "machine-substrate unknown condition: {cond}")
            }
            Self::InvalidImmediate(msg) => write!(f, "machine-substrate invalid immediate: {msg}"),
            Self::AssemblyError(msg) => write!(f, "machine-substrate assembly error: {msg}"),
        }
    }
}

impl std::error::Error for MachineSubstrateError {}

const SUBSTRATE_PROVENANCE: Provenance = Provenance::new(None, "lisp-authored assembler substrate");

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
        MachineInst::AluRegReg { op, dst, src, .. } => format!(
            "(x86 alu-reg-reg {} {} {})",
            op.raw_name(),
            dst.raw_name(),
            src.raw_name()
        ),
        MachineInst::AluImm8 { op, dst, imm, .. } => {
            format!("(x86 alu-imm8 {} {} {imm})", op.raw_name(), dst.raw_name())
        }
        MachineInst::AluImm32 { op, dst, imm, .. } => {
            format!("(x86 alu-imm32 {} {} {imm})", op.raw_name(), dst.raw_name())
        }
        MachineInst::Lea {
            dst, base, disp, ..
        } => format!("(x86 lea {} {} {disp})", dst.raw_name(), base.raw_name()),
        MachineInst::MovRegReg { dst, src, .. } => {
            format!("(x86 mov-reg-reg {} {})", dst.raw_name(), src.raw_name())
        }
        MachineInst::MovStore {
            base, disp, src, ..
        } => format!(
            "(x86 mov-store {} {disp} {})",
            base.raw_name(),
            src.raw_name()
        ),
        MachineInst::MovLoad {
            dst, base, disp, ..
        } => format!(
            "(x86 mov-load {} {} {disp})",
            dst.raw_name(),
            base.raw_name()
        ),
        MachineInst::MovImm64 { dst, imm, .. } => {
            format!("(x86 mov-imm64 {} {imm})", dst.raw_name())
        }
        MachineInst::TestRegReg { reg1, reg2, .. } => {
            format!("(x86 test-reg-reg {} {})", reg1.raw_name(), reg2.raw_name())
        }
        MachineInst::PushReg { reg, .. } => format!("(x86 push-reg {})", reg.raw_name()),
        MachineInst::PopReg { reg, .. } => format!("(x86 pop-reg {})", reg.raw_name()),
        MachineInst::Syscall { .. } => "(x86 syscall)".to_string(),
        MachineInst::JmpRel32 { disp, .. } => format!("(x86 jmp-rel32 {disp})"),
        MachineInst::JccRel32 { cond, disp, .. } => {
            format!("(x86 jcc-rel32 {} {disp})", cond.name())
        }
        MachineInst::CallRel32 { disp, .. } => format!("(x86 call-rel32 {disp})"),
        MachineInst::Nop { .. } => "(x86 nop)".to_string(),
        MachineInst::Ret { .. } => "(x86 ret)".to_string(),
        MachineInst::Vzeroupper { .. } => "(x86 vzeroupper)".to_string(),
        MachineInst::VmovdquLoad {
            dst, base, disp, ..
        } => format!(
            "(x86 vmovdqu-load {} {} {disp})",
            dst.name(),
            base.raw_name()
        ),
        MachineInst::VmovdquStore {
            base, disp, src, ..
        } => format!(
            "(x86 vmovdqu-store {} {disp} {})",
            base.raw_name(),
            src.name()
        ),
        MachineInst::Vpaddd {
            dst, src1, src2, ..
        } => format!(
            "(x86 vpaddd {} {} {})",
            dst.name(),
            src1.name(),
            src2.name()
        ),
        MachineInst::VmovdGprToXmm { dst, src, .. } => {
            format!("(x86 vmovd-gpr-to-xmm {} {})", dst.name(), src.raw_name())
        }
        MachineInst::Vpbroadcastd { dst, src, .. } => {
            format!("(x86 vpbroadcastd {} {})", dst.name(), src.name())
        }
        MachineInst::MovLoad32 {
            dst, base, disp, ..
        } => format!(
            "(x86 mov-load32 {} {} {disp})",
            dst.raw_name(),
            base.raw_name()
        ),
        MachineInst::MovStore32 {
            base, disp, src, ..
        } => format!(
            "(x86 mov-store32 {} {disp} {})",
            base.raw_name(),
            src.raw_name()
        ),
        MachineInst::Alu32RegReg { op, dst, src, .. } => format!(
            "(x86 alu32-reg-reg {} {} {})",
            op.raw_name(),
            dst.raw_name(),
            src.raw_name()
        ),
    }
}

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

pub fn program_to_sexp(items: &[MachineItem]) -> String {
    items
        .iter()
        .map(item_to_sexp)
        .collect::<Vec<_>>()
        .join("\n")
}

fn expect_symbol(expr: &Expr, context: &str) -> Result<String, MachineSubstrateError> {
    match expr {
        Expr::Symbol(symbol) => Ok(symbol.clone()),
        other => Err(MachineSubstrateError::MalformedForm(format!(
            "expected symbol in {context}, found {other:?}"
        ))),
    }
}

fn expect_integer(expr: &Expr, context: &str) -> Result<i64, MachineSubstrateError> {
    match expr {
        Expr::Integer(integer) => Ok(*integer),
        other => Err(MachineSubstrateError::MalformedForm(format!(
            "expected integer in {context}, found {other:?}"
        ))),
    }
}

fn checked_u8(value: i64, context: &str) -> Result<u8, MachineSubstrateError> {
    u8::try_from(value).map_err(|_| {
        MachineSubstrateError::InvalidImmediate(format!(
            "{context} must fit unsigned 8-bit range, got {value}"
        ))
    })
}

fn checked_i8(value: i64, context: &str) -> Result<i8, MachineSubstrateError> {
    i8::try_from(value).map_err(|_| {
        MachineSubstrateError::InvalidImmediate(format!(
            "{context} must fit signed 8-bit range, got {value}"
        ))
    })
}

fn checked_i32(value: i64, context: &str) -> Result<i32, MachineSubstrateError> {
    i32::try_from(value).map_err(|_| {
        MachineSubstrateError::InvalidImmediate(format!(
            "{context} must fit signed 32-bit range, got {value}"
        ))
    })
}

fn parse_reg(expr: &Expr) -> Result<X86Reg, MachineSubstrateError> {
    let symbol = expect_symbol(expr, "register operand")?;
    X86Reg::from_name(&symbol).ok_or_else(|| MachineSubstrateError::UnknownRegister(symbol))
}

fn parse_alu_op(expr: &Expr) -> Result<AluOp, MachineSubstrateError> {
    let symbol = expect_symbol(expr, "ALU operation")?;
    AluOp::from_name(&symbol).ok_or_else(|| MachineSubstrateError::UnknownOp(symbol))
}

fn parse_cond_code(expr: &Expr) -> Result<CondCode, MachineSubstrateError> {
    let symbol = expect_symbol(expr, "condition code")?;
    CondCode::from_name(&symbol).ok_or_else(|| MachineSubstrateError::UnknownCondition(symbol))
}

pub fn item_from_sexp(expr: &Expr) -> Result<MachineItem, MachineSubstrateError> {
    let Expr::List(list) = expr else {
        return Err(MachineSubstrateError::MalformedForm(format!(
            "expected target list (x86 ...), found {expr:?}"
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

    let op = expect_symbol(&list[1], "machine item operator")?;
    match op.as_str() {
        "label" => {
            if list.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 label <name>)".to_string(),
                ));
            }
            Ok(MachineItem::Label(expect_symbol(&list[2], "label name")?))
        }
        "jmp-label" => {
            if list.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 jmp-label <target>)".to_string(),
                ));
            }
            Ok(MachineItem::JmpLabel {
                target: expect_symbol(&list[2], "jump target")?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "jcc-label" => {
            if list.len() != 4 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 jcc-label <cond> <target>)".to_string(),
                ));
            }
            Ok(MachineItem::JccLabel {
                cond: parse_cond_code(&list[2])?,
                target: expect_symbol(&list[3], "conditional jump target")?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "call-label" => {
            if list.len() != 3 {
                return Err(MachineSubstrateError::MalformedForm(
                    "usage: (x86 call-label <target>)".to_string(),
                ));
            }
            Ok(MachineItem::CallLabel {
                target: expect_symbol(&list[2], "call target")?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        _ => Ok(MachineItem::Inst(inst_from_elements(&op, &list[2..])?)),
    }
}

fn inst_from_elements(op: &str, args: &[Expr]) -> Result<MachineInst, MachineSubstrateError> {
    match op {
        "rdtsc" => {
            expect_arity(args, 0, "(x86 rdtsc)")?;
            Ok(MachineInst::Rdtsc {
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "shl-imm" | "shr-imm" | "sar-imm" => {
            expect_arity(args, 2, &format!("(x86 {op} <reg> <imm8>)"))?;
            let reg = parse_reg(&args[0])?;
            let imm = checked_u8(
                expect_integer(&args[1], "shift immediate")?,
                "shift immediate",
            )?;
            let provenance = SUBSTRATE_PROVENANCE;
            Ok(match op {
                "shl-imm" => MachineInst::ShlImm {
                    reg,
                    imm,
                    provenance,
                },
                "shr-imm" => MachineInst::ShrImm {
                    reg,
                    imm,
                    provenance,
                },
                _ => MachineInst::SarImm {
                    reg,
                    imm,
                    provenance,
                },
            })
        }
        "alu-reg-reg" => {
            expect_arity(args, 3, "(x86 alu-reg-reg <op> <dst> <src>)")?;
            Ok(MachineInst::AluRegReg {
                op: parse_alu_op(&args[0])?,
                dst: parse_reg(&args[1])?,
                src: parse_reg(&args[2])?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "alu-imm8" => {
            expect_arity(args, 3, "(x86 alu-imm8 <op> <dst> <imm8>)")?;
            let value = expect_integer(&args[2], "ALU imm8")?;
            Ok(MachineInst::AluImm8 {
                op: parse_alu_op(&args[0])?,
                dst: parse_reg(&args[1])?,
                imm: checked_i8(value, "ALU imm8")?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "alu-imm32" => {
            expect_arity(args, 3, "(x86 alu-imm32 <op> <dst> <imm32>)")?;
            let value = expect_integer(&args[2], "ALU imm32")?;
            Ok(MachineInst::AluImm32 {
                op: parse_alu_op(&args[0])?,
                dst: parse_reg(&args[1])?,
                imm: checked_i32(value, "ALU imm32")?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "lea" => {
            expect_arity(args, 3, "(x86 lea <dst> <base> <disp>)")?;
            let value = expect_integer(&args[2], "LEA displacement")?;
            Ok(MachineInst::Lea {
                dst: parse_reg(&args[0])?,
                base: parse_reg(&args[1])?,
                disp: checked_i32(value, "LEA displacement")?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "mov-reg-reg" => {
            expect_arity(args, 2, "(x86 mov-reg-reg <dst> <src>)")?;
            Ok(MachineInst::MovRegReg {
                dst: parse_reg(&args[0])?,
                src: parse_reg(&args[1])?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "mov-store" => {
            expect_arity(args, 3, "(x86 mov-store <base> <disp> <src>)")?;
            let value = expect_integer(&args[1], "store displacement")?;
            Ok(MachineInst::MovStore {
                base: parse_reg(&args[0])?,
                disp: checked_i32(value, "store displacement")?,
                src: parse_reg(&args[2])?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "mov-load" => {
            expect_arity(args, 3, "(x86 mov-load <dst> <base> <disp>)")?;
            let value = expect_integer(&args[2], "load displacement")?;
            Ok(MachineInst::MovLoad {
                dst: parse_reg(&args[0])?,
                base: parse_reg(&args[1])?,
                disp: checked_i32(value, "load displacement")?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "mov-imm64" => {
            expect_arity(args, 2, "(x86 mov-imm64 <dst> <imm64>)")?;
            let value = expect_integer(&args[1], "imm64")?;
            Ok(MachineInst::MovImm64 {
                dst: parse_reg(&args[0])?,
                // Lisp integers in the current parser are i64. Preserve their exact
                // two's-complement 64-bit machine representation rather than narrowing.
                imm: value as u64,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "test-reg-reg" => {
            expect_arity(args, 2, "(x86 test-reg-reg <reg1> <reg2>)")?;
            Ok(MachineInst::TestRegReg {
                reg1: parse_reg(&args[0])?,
                reg2: parse_reg(&args[1])?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "push-reg" => {
            expect_arity(args, 1, "(x86 push-reg <reg>)")?;
            Ok(MachineInst::PushReg {
                reg: parse_reg(&args[0])?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "pop-reg" => {
            expect_arity(args, 1, "(x86 pop-reg <reg>)")?;
            Ok(MachineInst::PopReg {
                reg: parse_reg(&args[0])?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "syscall" => {
            expect_arity(args, 0, "(x86 syscall)")?;
            Ok(MachineInst::Syscall {
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "jmp-rel32" => {
            expect_arity(args, 1, "(x86 jmp-rel32 <disp>)")?;
            let value = expect_integer(&args[0], "jump displacement")?;
            Ok(MachineInst::JmpRel32 {
                disp: checked_i32(value, "jump displacement")?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "jcc-rel32" => {
            expect_arity(args, 2, "(x86 jcc-rel32 <cond> <disp>)")?;
            let value = expect_integer(&args[1], "conditional jump displacement")?;
            Ok(MachineInst::JccRel32 {
                cond: parse_cond_code(&args[0])?,
                disp: checked_i32(value, "conditional jump displacement")?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "call-rel32" => {
            expect_arity(args, 1, "(x86 call-rel32 <disp>)")?;
            let value = expect_integer(&args[0], "call displacement")?;
            Ok(MachineInst::CallRel32 {
                disp: checked_i32(value, "call displacement")?,
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "nop" => {
            expect_arity(args, 0, "(x86 nop)")?;
            Ok(MachineInst::Nop {
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        "ret" => {
            expect_arity(args, 0, "(x86 ret)")?;
            Ok(MachineInst::Ret {
                provenance: SUBSTRATE_PROVENANCE,
            })
        }
        other => Err(MachineSubstrateError::UnknownOp(other.to_string())),
    }
}

fn expect_arity(args: &[Expr], expected: usize, usage: &str) -> Result<(), MachineSubstrateError> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(MachineSubstrateError::MalformedForm(format!(
            "usage: {usage}; expected {expected} operand(s), got {}",
            args.len()
        )))
    }
}

pub fn parse_machine_program(source: &str) -> Result<Vec<MachineItem>, MachineSubstrateError> {
    let exprs =
        parse(source).map_err(|error| MachineSubstrateError::ParseError(error.to_string()))?;
    exprs.iter().map(item_from_sexp).collect()
}

pub fn assemble_sexp_program(source: &str) -> Result<Vec<u8>, MachineSubstrateError> {
    let items = parse_machine_program(source)?;
    assemble_program(&items).map_err(MachineSubstrateError::AssemblyError)
}

pub fn expand_macro_atoms(source: &str) -> Result<Vec<MachineItem>, MachineSubstrateError> {
    let exprs =
        parse(source).map_err(|error| MachineSubstrateError::ParseError(error.to_string()))?;
    let mut expander = MacroExpander::new();
    let expanded = expander
        .process(&exprs)
        .map_err(|error| MachineSubstrateError::MalformedForm(error.to_string()))?;
    expanded.iter().map(item_from_sexp).collect()
}
