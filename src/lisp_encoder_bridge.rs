//! Lisp Encoder Bridge: boundary between CML optimizer/synthesizer and
//! the upstream `my-lisp`-owned physical x86-64 machine encoder (`lib/machine/encoding/x86-64.lisp`).
//!
//! # Мовна межа та авторитет (Authority Boundary)
//!
//! Згідно з архітектурним контрактом CML та мови `my-lisp`:
//! - `my-lisp` володіє семантикою мови та фізичним кодуванням байтів x86-64
//!   (`lib/machine/encoding/x86-64.lisp`, введений у PR `my-lisp#118`).
//! - `CML` є оптимізатором і планувальником (synthesizer): він вирішує вибір інструкцій
//!   та їх порядок, але не винаходить власну мовну семантику і не підміняє авторитет
//!   кодування байтів.
//!
//! Цей модуль проектує структуровані інструкції [`MachineInst`] та програми [`MachineItem`]
//! у канонічні S-вирази викликів Lisp-кодувальника, а також розбирає згенерований Lisp'ом
//! список байтів для потрійної перевірки:
//! `Lisp-кодувальник (A) == CML прямий кодувальник (B) == GNU as оракул (C)`.

use crate::machine_inst::{AluOp, MachineInst, MachineItem, X86Reg};

/// Exact commit SHA of upstream `juv4uk/my-lisp` providing the x86-64 encoder.
pub const PINNED_MYLISP_COMMIT: &str = "d4ad7e7c7717a610599875ffb90123b713ac05c7";

/// Bridge conversion or parsing error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    /// Instruction is outside the admitted slice for the Lisp encoder bridge.
    UnsupportedInstruction(String),
    /// Register is not admitted by the target architecture specification.
    UnadmittedRegister(String),
    /// Machine program or item sequence cannot be encoded via the bridge.
    InvalidProgram(String),
    /// Failed to parse Lisp byte list textual representation.
    ParseError(String),
    /// Execution error during Lisp oracle evaluation.
    ExecutionError(String),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedInstruction(msg) => write!(f, "unsupported instruction: {msg}"),
            Self::UnadmittedRegister(msg) => write!(f, "unadmitted register: {msg}"),
            Self::InvalidProgram(msg) => write!(f, "invalid program: {msg}"),
            Self::ParseError(msg) => write!(f, "parse error: {msg}"),
            Self::ExecutionError(msg) => write!(f, "execution error: {msg}"),
        }
    }
}

impl std::error::Error for BridgeError {}

/// Maps an x86-64 64-bit general-purpose register to the symbol admitted by `x86-reg-code`.
#[inline]
pub fn reg64_to_lisp_symbol(reg: X86Reg) -> Result<&'static str, BridgeError> {
    match reg {
        X86Reg::Rax => Ok("rax"),
        X86Reg::Rcx => Ok("rcx"),
        X86Reg::Rdx => Ok("rdx"),
        X86Reg::Rbx => Ok("rbx"),
        X86Reg::Rsp => Ok("rsp"),
        X86Reg::Rbp => Ok("rbp"),
        X86Reg::Rsi => Ok("rsi"),
        X86Reg::Rdi => Ok("rdi"),
        X86Reg::R8 => Ok("r8"),
        X86Reg::R9 => Ok("r9"),
        X86Reg::R10 => Ok("r10"),
        X86Reg::R11 => Ok("r11"),
        X86Reg::R12 => Ok("r12"),
        X86Reg::R13 => Ok("r13"),
        X86Reg::R14 => Ok("r14"),
        X86Reg::R15 => Ok("r15"),
    }
}

/// Converts an admitted scalar [`MachineInst`] to a Lisp encoder expression.
///
/// Admitted scalar subset in slice #52:
/// - `MovImm64 { dst, imm, .. }` -> `(x86-encode-mov-r64-imm64 '{dst} {imm})`
/// - `AluRegReg { op: AluOp::Add, dst, src, .. }` -> `(x86-encode-add-r64-r64 '{dst} '{src})`
/// - `Ret { .. }` -> `(x86-encode-ret)`
///
/// All other instructions fail closed.
pub fn inst_to_lisp_encoder_call(inst: &MachineInst) -> Result<String, BridgeError> {
    match inst {
        MachineInst::MovImm64 { dst, imm, .. } => {
            let reg_sym = reg64_to_lisp_symbol(*dst)?;
            Ok(format!("(x86-encode-mov-r64-imm64 '{reg_sym} {imm})"))
        }
        MachineInst::AluRegReg {
            op: AluOp::Add,
            dst,
            src,
            ..
        } => {
            let dst_sym = reg64_to_lisp_symbol(*dst)?;
            let src_sym = reg64_to_lisp_symbol(*src)?;
            Ok(format!("(x86-encode-add-r64-r64 '{dst_sym} '{src_sym})"))
        }
        MachineInst::Ret { .. } => Ok("(x86-encode-ret)".to_string()),
        other => Err(BridgeError::UnsupportedInstruction(format!(
            "instruction {:?} is not admitted in the #52 scalar Lisp encoder bridge (only mov-imm64, add-r64-r64, ret admitted)",
            other
        ))),
    }
}

/// Converts a sequence of [`MachineItem`] into a single `(x86-encode-program (list ...))` form.
///
/// Only sequences composed exclusively of admitted [`MachineItem::Inst`] are accepted.
/// Labels, unresolved jumps, or calls fail closed until their resolution is admitted.
pub fn items_to_lisp_encoder_program(items: &[MachineItem]) -> Result<String, BridgeError> {
    if items.is_empty() {
        return Ok("(quote ())".to_string());
    }
    let mut calls = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        match item {
            MachineItem::Inst(inst) => {
                calls.push(inst_to_lisp_encoder_call(inst)?);
            }
            other => {
                return Err(BridgeError::InvalidProgram(format!(
                    "item at index {idx} ({other:?}) is not an admitted instruction; labels and non-inst items must be lowered/resolved before encoder invocation"
                )));
            }
        }
    }

    Ok(format!(
        "(x86-encode-program\n  (list\n    {}\n  ))",
        calls.join("\n    ")
    ))
}

/// Parses the textual s-expression output of a Lisp byte list (e.g. `"(72 184 10 0 0 ...)"`) into `Vec<u8>`.
pub fn parse_lisp_byte_list_str(text: &str) -> Result<Vec<u8>, BridgeError> {
    let trimmed = text.trim();
    if trimmed == "()" || trimmed == "(quote ())" || trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let inner = trimmed
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| {
            BridgeError::ParseError(format!("expected parenthesized list, found: {trimmed}"))
        })?;

    let mut bytes = Vec::new();
    for token in inner.split_whitespace() {
        let byte_val: u8 = token.parse().map_err(|_| {
            BridgeError::ParseError(format!(
                "token '{token}' is not an unsigned 8-bit integer (0..255) in list: {trimmed}"
            ))
        })?;
        bytes.push(byte_val);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_inst::{AluOp, Provenance};

    fn test_prov() -> Provenance {
        Provenance::new(None, "test")
    }

    #[test]
    fn admitted_instructions_produce_valid_lisp_forms() {
        let mov = MachineInst::MovImm64 {
            dst: X86Reg::Rax,
            imm: 42,
            provenance: test_prov(),
        };
        assert_eq!(
            inst_to_lisp_encoder_call(&mov).unwrap(),
            "(x86-encode-mov-r64-imm64 'rax 42)"
        );

        let add = MachineInst::AluRegReg {
            op: AluOp::Add,
            dst: X86Reg::Rax,
            src: X86Reg::Rcx,
            provenance: test_prov(),
        };
        assert_eq!(
            inst_to_lisp_encoder_call(&add).unwrap(),
            "(x86-encode-add-r64-r64 'rax 'rcx)"
        );

        let ret = MachineInst::Ret {
            provenance: test_prov(),
        };
        assert_eq!(inst_to_lisp_encoder_call(&ret).unwrap(), "(x86-encode-ret)");
    }

    #[test]
    fn unadmitted_instructions_fail_closed() {
        let sub = MachineInst::AluRegReg {
            op: AluOp::Sub,
            dst: X86Reg::Rax,
            src: X86Reg::Rcx,
            provenance: test_prov(),
        };
        assert!(matches!(
            inst_to_lisp_encoder_call(&sub),
            Err(BridgeError::UnsupportedInstruction(_))
        ));

        let rdtsc = MachineInst::Rdtsc {
            provenance: test_prov(),
        };
        assert!(matches!(
            inst_to_lisp_encoder_call(&rdtsc),
            Err(BridgeError::UnsupportedInstruction(_))
        ));
    }

    #[test]
    fn program_wrapping_and_item_admission() {
        let items = vec![
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rax,
                imm: 10,
                provenance: test_prov(),
            }),
            MachineItem::Inst(MachineInst::Ret {
                provenance: test_prov(),
            }),
        ];
        let prog = items_to_lisp_encoder_program(&items).unwrap();
        assert!(prog.starts_with("(x86-encode-program"));
        assert!(prog.contains("(x86-encode-mov-r64-imm64 'rax 10)"));
        assert!(prog.contains("(x86-encode-ret)"));

        let unadmitted_label = vec![
            MachineItem::Label("start".to_string()),
            MachineItem::Inst(MachineInst::Ret {
                provenance: test_prov(),
            }),
        ];
        assert!(matches!(
            items_to_lisp_encoder_program(&unadmitted_label),
            Err(BridgeError::InvalidProgram(_))
        ));
    }

    #[test]
    fn parse_lisp_byte_list_handles_empty_and_valid() {
        assert_eq!(parse_lisp_byte_list_str("()").unwrap(), Vec::<u8>::new());
        assert_eq!(parse_lisp_byte_list_str(" ( ) ").unwrap(), Vec::<u8>::new());
        assert_eq!(
            parse_lisp_byte_list_str("(72 184 42 0 0 0 0 0 0 0 195)").unwrap(),
            vec![72, 184, 42, 0, 0, 0, 0, 0, 0, 0, 195]
        );
        assert!(parse_lisp_byte_list_str("72 184").is_err());
        assert!(parse_lisp_byte_list_str("(256)").is_err());
        assert!(parse_lisp_byte_list_str("(-1)").is_err());
        assert!(parse_lisp_byte_list_str("(not_a_number)").is_err());
    }
}
