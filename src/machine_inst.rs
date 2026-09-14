//! `MachineInst`: structured machine instruction layer between backend-neutral
//! compiler IR (`Ir::MachinePrim`) and physical target projections (GNU assembly text
//! or direct machine bytes).
//!
//! # Architecture
//!
//! ```text
//! CML compiler target mechanism (e.g. read-cycle-counter / RDTSC)
//!         │
//!         ▼
//! CML backend-neutral IR: Ir::MachinePrim(MachineOp::Rdtsc)
//!         │
//!         ▼
//! target selection: select_machine_primitive(MachineOp, ...)
//!         │
//!         ▼
//! Vec<MachineInst> (structured instruction data with provenance)
//!         ├───► Projection A: print_gnu_asm() -> text string for GNU as
//!         ├───► Projection B: encode_bytes()   -> direct x86-64 machine code
//!         └───► Oracle verification: direct bytes == GNU as objdump bytes
//! ```

use crate::ir::{Ir, MachineOp, PrimOp};

/// Target x86-64 64-bit general-purpose registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X86Reg {
    Rax,
    Rcx,
    Rdx,
    Rbx,
    Rsp,
    Rbp,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

impl X86Reg {
    /// Hardware register encoding index (0..15).
    #[inline]
    pub const fn number(self) -> u8 {
        match self {
            Self::Rax => 0,
            Self::Rcx => 1,
            Self::Rdx => 2,
            Self::Rbx => 3,
            Self::Rsp => 4,
            Self::Rbp => 5,
            Self::Rsi => 6,
            Self::Rdi => 7,
            Self::R8 => 8,
            Self::R9 => 9,
            Self::R10 => 10,
            Self::R11 => 11,
            Self::R12 => 12,
            Self::R13 => 13,
            Self::R14 => 14,
            Self::R15 => 15,
        }
    }

    /// Whether this register requires REX extension bit (R8..R15).
    #[inline]
    pub const fn is_extended(self) -> bool {
        self.number() >= 8
    }

    /// Standard GNU assembler register name.
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rax => "%rax",
            Self::Rcx => "%rcx",
            Self::Rdx => "%rdx",
            Self::Rbx => "%rbx",
            Self::Rsp => "%rsp",
            Self::Rbp => "%rbp",
            Self::Rsi => "%rsi",
            Self::Rdi => "%rdi",
            Self::R8 => "%r8",
            Self::R9 => "%r9",
            Self::R10 => "%r10",
            Self::R11 => "%r11",
            Self::R12 => "%r12",
            Self::R13 => "%r13",
            Self::R14 => "%r14",
            Self::R15 => "%r15",
        }
    }

    /// Register name without '%' prefix suitable for S-expression data.
    #[inline]
    pub const fn raw_name(self) -> &'static str {
        match self {
            Self::Rax => "rax",
            Self::Rcx => "rcx",
            Self::Rdx => "rdx",
            Self::Rbx => "rbx",
            Self::Rsp => "rsp",
            Self::Rbp => "rbp",
            Self::Rsi => "rsi",
            Self::Rdi => "rdi",
            Self::R8 => "r8",
            Self::R9 => "r9",
            Self::R10 => "r10",
            Self::R11 => "r11",
            Self::R12 => "r12",
            Self::R13 => "r13",
            Self::R14 => "r14",
            Self::R15 => "r15",
        }
    }

    /// Parse register from name string (with or without '%' prefix).
    pub fn from_name(name: &str) -> Option<Self> {
        let clean = name.strip_prefix('%').unwrap_or(name);
        match clean {
            "rax" => Some(Self::Rax),
            "rcx" => Some(Self::Rcx),
            "rdx" => Some(Self::Rdx),
            "rbx" => Some(Self::Rbx),
            "rsp" => Some(Self::Rsp),
            "rbp" => Some(Self::Rbp),
            "rsi" => Some(Self::Rsi),
            "rdi" => Some(Self::Rdi),
            "r8" => Some(Self::R8),
            "r9" => Some(Self::R9),
            "r10" => Some(Self::R10),
            "r11" => Some(Self::R11),
            "r12" => Some(Self::R12),
            "r13" => Some(Self::R13),
            "r14" => Some(Self::R14),
            "r15" => Some(Self::R15),
            _ => None,
        }
    }
}

/// Standard x86-64 symmetric 64-bit arithmetic and logic operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AluOp {
    Add = 0,
    Or = 1,
    Adc = 2,
    Sbb = 3,
    And = 4,
    Sub = 5,
    Xor = 6,
    Cmp = 7,
}

impl AluOp {
    /// Opcode byte for `OP r/m64, r64` (e.g. `0x01` for ADD, `0x29` for SUB).
    #[inline]
    pub const fn opcode_reg_reg(self) -> u8 {
        (self as u8) * 8 + 0x01
    }

    /// ModR/M `reg` field digit (/0 to /7) used when combining with opcode 0x83 / 0x81 for immediates.
    #[inline]
    pub const fn modrm_digit(self) -> u8 {
        self as u8
    }

    /// Standard GNU mnemonic for 64-bit quadword form (e.g. "addq", "subq").
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Add => "addq",
            Self::Or => "orq",
            Self::Adc => "adcq",
            Self::Sbb => "sbbq",
            Self::And => "andq",
            Self::Sub => "subq",
            Self::Xor => "xorq",
            Self::Cmp => "cmpq",
        }
    }

    /// Base mnemonic symbol without size suffix for S-expression data.
    #[inline]
    pub const fn raw_name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Or => "or",
            Self::Adc => "adc",
            Self::Sbb => "sbb",
            Self::And => "and",
            Self::Sub => "sub",
            Self::Xor => "xor",
            Self::Cmp => "cmp",
        }
    }

    /// Parse ALU operation from mnemonic string.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "add" | "addq" => Some(Self::Add),
            "or" | "orq" => Some(Self::Or),
            "adc" | "adcq" => Some(Self::Adc),
            "sbb" | "sbbq" => Some(Self::Sbb),
            "and" | "andq" => Some(Self::And),
            "sub" | "subq" => Some(Self::Sub),
            "xor" | "xorq" => Some(Self::Xor),
            "cmp" | "cmpq" => Some(Self::Cmp),
            _ => None,
        }
    }
}

/// x86 condition codes for conditional jumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CondCode {
    Overflow = 0x0,
    NotOverflow = 0x1,
    Below = 0x2,
    AboveEqual = 0x3,
    Equal = 0x4,
    NotEqual = 0x5,
    BelowEqual = 0x6,
    Above = 0x7,
    Sign = 0x8,
    NotSign = 0x9,
    Parity = 0xA,
    NotParity = 0xB,
    Less = 0xC,
    GreaterEqual = 0xD,
    LessEqual = 0xE,
    Greater = 0xF,
}

impl CondCode {
    #[inline]
    pub const fn mnemonic_suffix(self) -> &'static str {
        match self {
            Self::Overflow => "o",
            Self::NotOverflow => "no",
            Self::Below => "b",
            Self::AboveEqual => "ae",
            Self::Equal => "e",
            Self::NotEqual => "ne",
            Self::BelowEqual => "be",
            Self::Above => "a",
            Self::Sign => "s",
            Self::NotSign => "ns",
            Self::Parity => "p",
            Self::NotParity => "np",
            Self::Less => "l",
            Self::GreaterEqual => "ge",
            Self::LessEqual => "le",
            Self::Greater => "g",
        }
    }

    /// Canonical name for S-expression representation.
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Overflow => "overflow",
            Self::NotOverflow => "not-overflow",
            Self::Below => "below",
            Self::AboveEqual => "above-equal",
            Self::Equal => "equal",
            Self::NotEqual => "not-equal",
            Self::BelowEqual => "below-equal",
            Self::Above => "above",
            Self::Sign => "sign",
            Self::NotSign => "not-sign",
            Self::Parity => "parity",
            Self::NotParity => "not-parity",
            Self::Less => "less",
            Self::GreaterEqual => "greater-equal",
            Self::LessEqual => "less-equal",
            Self::Greater => "greater",
        }
    }

    /// Parse condition code from string.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "overflow" | "o" => Some(Self::Overflow),
            "not-overflow" | "no" => Some(Self::NotOverflow),
            "below" | "b" | "c" => Some(Self::Below),
            "above-equal" | "ae" | "nc" => Some(Self::AboveEqual),
            "equal" | "e" | "zero" | "z" => Some(Self::Equal),
            "not-equal" | "ne" | "not-zero" | "nz" => Some(Self::NotEqual),
            "below-equal" | "be" => Some(Self::BelowEqual),
            "above" | "a" => Some(Self::Above),
            "sign" | "s" => Some(Self::Sign),
            "not-sign" | "ns" => Some(Self::NotSign),
            "parity" | "p" => Some(Self::Parity),
            "not-parity" | "np" => Some(Self::NotParity),
            "less" | "l" => Some(Self::Less),
            "greater-equal" | "ge" => Some(Self::GreaterEqual),
            "less-equal" | "le" => Some(Self::LessEqual),
            "greater" | "g" => Some(Self::Greater),
            _ => None,
        }
    }
}

/// Provenance metadata tracking which semantic identity / pass produced this machine instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub semantic_id: Option<&'static str>,
    pub description: &'static str,
}

impl Provenance {
    pub const fn new(semantic_id: Option<&'static str>, description: &'static str) -> Self {
        Self {
            semantic_id,
            description,
        }
    }
}

/// Canonical structured machine instruction for target execution.
///
/// Each instruction is a first-class data structure, completely decoupled from
/// string formatting or source-level language spellings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineInst {
    /// Read Time-Stamp Counter into EDX:EAX.
    Rdtsc { provenance: Provenance },

    /// Logical shift left 64-bit register by immediate 8-bit count: `shlq $imm, %reg`.
    ShlImm {
        reg: X86Reg,
        imm: u8,
        provenance: Provenance,
    },

    /// Symmetric ALU operation between two 64-bit registers: `OP %src, %dst`.
    AluRegReg {
        op: AluOp,
        dst: X86Reg,
        src: X86Reg,
        provenance: Provenance,
    },

    /// Symmetric ALU operation between 64-bit register and immediate 8-bit sign-extended value: `OP $imm, %dst`.
    AluImm8 {
        op: AluOp,
        dst: X86Reg,
        imm: i8,
        provenance: Provenance,
    },

    /// Symmetric ALU operation between 64-bit register and immediate 32-bit value: `OP $imm, %dst`.
    AluImm32 {
        op: AluOp,
        dst: X86Reg,
        imm: i32,
        provenance: Provenance,
    },

    /// Load effective address into 64-bit register: `leaq disp(%base), %dst`.
    Lea {
        dst: X86Reg,
        base: X86Reg,
        disp: i32,
        provenance: Provenance,
    },

    /// Move 64-bit register to 64-bit register: `movq %src, %dst`.
    MovRegReg {
        dst: X86Reg,
        src: X86Reg,
        provenance: Provenance,
    },

    /// Store 64-bit register into memory displacement: `movq %src, disp(%base)`.
    MovStore {
        base: X86Reg,
        disp: i32,
        src: X86Reg,
        provenance: Provenance,
    },

    /// Load 64-bit register from memory displacement: `movq disp(%base), %dst`.
    MovLoad {
        dst: X86Reg,
        base: X86Reg,
        disp: i32,
        provenance: Provenance,
    },

    /// Move 64-bit immediate into register: `movabsq $imm, %dst`.
    MovImm64 {
        dst: X86Reg,
        imm: u64,
        provenance: Provenance,
    },

    /// Test two 64-bit registers: `testq %reg2, %reg1`.
    TestRegReg {
        reg1: X86Reg,
        reg2: X86Reg,
        provenance: Provenance,
    },

    /// Arithmetic shift right 64-bit register by immediate 8-bit count: `sarq $imm, %reg`.
    SarImm {
        reg: X86Reg,
        imm: u8,
        provenance: Provenance,
    },

    /// Logical shift right 64-bit register by immediate 8-bit count: `shrq $imm, %reg`.
    ShrImm {
        reg: X86Reg,
        imm: u8,
        provenance: Provenance,
    },

    /// Push 64-bit register onto stack: `pushq %reg`.
    PushReg { reg: X86Reg, provenance: Provenance },

    /// Pop 64-bit register from stack: `popq %reg`.
    PopReg { reg: X86Reg, provenance: Provenance },

    /// Fast system call invocation: `syscall`.
    Syscall { provenance: Provenance },

    /// Unconditional near jump with 32-bit relative displacement: `jmp rel32`.
    JmpRel32 { disp: i32, provenance: Provenance },

    /// Conditional near jump with 32-bit relative displacement: `j<cond> rel32`.
    JccRel32 {
        cond: CondCode,
        disp: i32,
        provenance: Provenance,
    },

    /// Near procedure call with 32-bit relative displacement: `call rel32`.
    CallRel32 { disp: i32, provenance: Provenance },

    /// No operation: `nop`.
    Nop { provenance: Provenance },

    /// Near return to calling procedure: `ret`.
    Ret { provenance: Provenance },
}

impl MachineInst {
    /// Returns the instruction's provenance metadata.
    pub fn provenance(&self) -> &Provenance {
        match self {
            Self::Rdtsc { provenance }
            | Self::ShlImm { provenance, .. }
            | Self::SarImm { provenance, .. }
            | Self::ShrImm { provenance, .. }
            | Self::PushReg { provenance, .. }
            | Self::PopReg { provenance, .. }
            | Self::Syscall { provenance }
            | Self::JmpRel32 { provenance, .. }
            | Self::JccRel32 { provenance, .. }
            | Self::CallRel32 { provenance, .. }
            | Self::AluRegReg { provenance, .. }
            | Self::AluImm8 { provenance, .. }
            | Self::AluImm32 { provenance, .. }
            | Self::Lea { provenance, .. }
            | Self::MovRegReg { provenance, .. }
            | Self::MovStore { provenance, .. }
            | Self::MovLoad { provenance, .. }
            | Self::MovImm64 { provenance, .. }
            | Self::TestRegReg { provenance, .. }
            | Self::Nop { provenance }
            | Self::Ret { provenance } => provenance,
        }
    }

    /// Projection A: Render instruction as standard GNU assembly text.
    pub fn print_gnu_asm(&self) -> String {
        match self {
            Self::Rdtsc { .. } => "rdtsc".to_string(),
            Self::ShlImm { reg, imm, .. } => format!("shlq ${imm}, {}", reg.name()),
            Self::SarImm { reg, imm, .. } => format!("sarq ${imm}, {}", reg.name()),
            Self::ShrImm { reg, imm, .. } => format!("shrq ${imm}, {}", reg.name()),
            Self::PushReg { reg, .. } => format!("pushq {}", reg.name()),
            Self::PopReg { reg, .. } => format!("popq {}", reg.name()),
            Self::Syscall { .. } => "syscall".to_string(),
            Self::JmpRel32 { disp, .. } => format!(".byte 0xe9; .long {disp}"),
            Self::JccRel32 { cond, disp, .. } => {
                format!(".byte 0x0f, 0x{:02x}; .long {disp}", 0x80 + (*cond as u8))
            }
            Self::CallRel32 { disp, .. } => format!(".byte 0xe8; .long {disp}"),
            Self::AluRegReg { op, dst, src, .. } => {
                format!("{} {}, {}", op.name(), src.name(), dst.name())
            }
            Self::AluImm8 { op, dst, imm, .. } => {
                format!("{} ${imm}, {}", op.name(), dst.name())
            }
            Self::AluImm32 { op, dst, imm, .. } => {
                format!("{} ${imm}, {}", op.name(), dst.name())
            }
            Self::Lea {
                dst, base, disp, ..
            } => {
                if *disp == 0 {
                    format!("leaq ({}), {}", base.name(), dst.name())
                } else {
                    format!("leaq {disp}({}), {}", base.name(), dst.name())
                }
            }
            Self::MovRegReg { dst, src, .. } => {
                format!("movq {}, {}", src.name(), dst.name())
            }
            Self::MovStore {
                base, disp, src, ..
            } => {
                if *disp == 0 {
                    format!("movq {}, ({})", src.name(), base.name())
                } else {
                    format!("movq {}, {disp}({})", src.name(), base.name())
                }
            }
            Self::MovLoad {
                dst, base, disp, ..
            } => {
                if *disp == 0 {
                    format!("movq ({}), {}", base.name(), dst.name())
                } else {
                    format!("movq {disp}({}), {}", base.name(), dst.name())
                }
            }
            Self::MovImm64 { dst, imm, .. } => format!("movabsq $0x{imm:016X}, {}", dst.name()),
            Self::TestRegReg { reg1, reg2, .. } => {
                format!("testq {}, {}", reg2.name(), reg1.name())
            }
            Self::Nop { .. } => "nop".to_string(),
            Self::Ret { .. } => "ret".to_string(),
        }
    }

    /// Projection B: Encode instruction directly into x86-64 physical machine bytes.
    pub fn encode_bytes(&self) -> Vec<u8> {
        match self {
            Self::Rdtsc { .. } => vec![0x0F, 0x31],

            Self::ShlImm { reg, imm, .. } => {
                let rex = 0x48 | if reg.is_extended() { 0x01 } else { 0x00 };
                let opcode = 0xC1;
                // ModR/M: mod=11 (register direct), reg=100 (/4 for SHL), rm=reg.number() & 7
                let modrm = (0b11 << 6) | (0b100 << 3) | (reg.number() & 0x07);
                vec![rex, opcode, modrm, *imm]
            }

            Self::SarImm { reg, imm, .. } => {
                let rex = 0x48 | if reg.is_extended() { 0x01 } else { 0x00 };
                let opcode = 0xC1;
                // ModR/M: mod=11 (register direct), reg=111 (/7 for SAR), rm=reg.number() & 7
                let modrm = (0b11 << 6) | (0b111 << 3) | (reg.number() & 0x07);
                vec![rex, opcode, modrm, *imm]
            }

            Self::ShrImm { reg, imm, .. } => {
                let rex = 0x48 | if reg.is_extended() { 0x01 } else { 0x00 };
                let opcode = 0xC1;
                // ModR/M: mod=11 (register direct), reg=101 (/5 for SHR), rm=reg.number() & 7
                let modrm = (0b11 << 6) | (0b101 << 3) | (reg.number() & 0x07);
                vec![rex, opcode, modrm, *imm]
            }

            Self::PushReg { reg, .. } => {
                if reg.is_extended() {
                    vec![0x41, 0x50 + (reg.number() & 0x07)]
                } else {
                    vec![0x50 + reg.number()]
                }
            }

            Self::PopReg { reg, .. } => {
                if reg.is_extended() {
                    vec![0x41, 0x58 + (reg.number() & 0x07)]
                } else {
                    vec![0x58 + reg.number()]
                }
            }

            Self::Syscall { .. } => vec![0x0F, 0x05],

            Self::JmpRel32 { disp, .. } => {
                let mut bytes = Vec::with_capacity(5);
                bytes.push(0xE9);
                bytes.extend_from_slice(&disp.to_le_bytes());
                bytes
            }

            Self::JccRel32 { cond, disp, .. } => {
                let mut bytes = Vec::with_capacity(6);
                bytes.push(0x0F);
                bytes.push(0x80 + (*cond as u8));
                bytes.extend_from_slice(&disp.to_le_bytes());
                bytes
            }

            Self::CallRel32 { disp, .. } => {
                let mut bytes = Vec::with_capacity(5);
                bytes.push(0xE8);
                bytes.extend_from_slice(&disp.to_le_bytes());
                bytes
            }

            Self::AluRegReg { op, dst, src, .. } => {
                let rex = 0x48
                    | if src.is_extended() { 0x04 } else { 0x00 }
                    | if dst.is_extended() { 0x01 } else { 0x00 };
                let opcode = op.opcode_reg_reg();
                let modrm = (0b11 << 6) | ((src.number() & 0x07) << 3) | (dst.number() & 0x07);
                vec![rex, opcode, modrm]
            }

            Self::AluImm8 { op, dst, imm, .. } => {
                let rex = 0x48 | if dst.is_extended() { 0x01 } else { 0x00 };
                let opcode = 0x83;
                let modrm = (0b11 << 6) | (op.modrm_digit() << 3) | (dst.number() & 0x07);
                vec![rex, opcode, modrm, *imm as u8]
            }

            Self::AluImm32 { op, dst, imm, .. } => {
                if *dst == X86Reg::Rax {
                    let rex = 0x48;
                    let opcode = (*op as u8) * 8 + 0x05;
                    let mut bytes = Vec::with_capacity(6);
                    bytes.push(rex);
                    bytes.push(opcode);
                    bytes.extend_from_slice(&imm.to_le_bytes());
                    bytes
                } else {
                    let rex = 0x48 | if dst.is_extended() { 0x01 } else { 0x00 };
                    let opcode = 0x81;
                    let modrm = (0b11 << 6) | (op.modrm_digit() << 3) | (dst.number() & 0x07);
                    let mut bytes = Vec::with_capacity(7);
                    bytes.push(rex);
                    bytes.push(opcode);
                    bytes.push(modrm);
                    bytes.extend_from_slice(&imm.to_le_bytes());
                    bytes
                }
            }

            Self::Lea {
                dst, base, disp, ..
            } => {
                let (rex, tail) =
                    encode_memory_access(dst.number(), *base, *disp, dst.is_extended());
                let opcode = 0x8D;
                let mut bytes = Vec::with_capacity(2 + tail.len());
                bytes.push(rex);
                bytes.push(opcode);
                bytes.extend_from_slice(&tail);
                bytes
            }

            Self::MovRegReg { dst, src, .. } => {
                let rex = 0x48
                    | if src.is_extended() { 0x04 } else { 0x00 }
                    | if dst.is_extended() { 0x01 } else { 0x00 };
                let opcode = 0x89;
                let modrm = (0b11 << 6) | ((src.number() & 0x07) << 3) | (dst.number() & 0x07);
                vec![rex, opcode, modrm]
            }

            Self::MovStore {
                base, disp, src, ..
            } => {
                let (rex, tail) =
                    encode_memory_access(src.number(), *base, *disp, src.is_extended());
                let opcode = 0x89;
                let mut bytes = Vec::with_capacity(2 + tail.len());
                bytes.push(rex);
                bytes.push(opcode);
                bytes.extend_from_slice(&tail);
                bytes
            }

            Self::MovLoad {
                dst, base, disp, ..
            } => {
                let (rex, tail) =
                    encode_memory_access(dst.number(), *base, *disp, dst.is_extended());
                let opcode = 0x8B;
                let mut bytes = Vec::with_capacity(2 + tail.len());
                bytes.push(rex);
                bytes.push(opcode);
                bytes.extend_from_slice(&tail);
                bytes
            }

            Self::MovImm64 { dst, imm, .. } => {
                let rex = 0x48 | if dst.is_extended() { 0x01 } else { 0x00 };
                let opcode = 0xB8 + (dst.number() & 0x07);
                let mut bytes = Vec::with_capacity(10);
                bytes.push(rex);
                bytes.push(opcode);
                bytes.extend_from_slice(&imm.to_le_bytes());
                bytes
            }

            Self::TestRegReg { reg1, reg2, .. } => {
                let rex = 0x48
                    | if reg2.is_extended() { 0x04 } else { 0x00 }
                    | if reg1.is_extended() { 0x01 } else { 0x00 };
                let opcode = 0x85;
                let modrm = (0b11 << 6) | ((reg2.number() & 0x07) << 3) | (reg1.number() & 0x07);
                vec![rex, opcode, modrm]
            }

            Self::Nop { .. } => vec![0x90],

            Self::Ret { .. } => vec![0xC3],
        }
    }
}

/// Helper to encode x86-64 memory addressing forms (`disp(%base)`) with ModR/M, optional SIB, and displacement.
fn encode_memory_access(modrm_reg: u8, base: X86Reg, disp: i32, reg_is_ext: bool) -> (u8, Vec<u8>) {
    let base_num = base.number();
    let base_is_ext = base.is_extended();
    let rex_b = if base_is_ext { 0x01 } else { 0x00 };
    let rex_r = if reg_is_ext { 0x04 } else { 0x00 };
    let rex = 0x48 | rex_r | rex_b;

    let needs_sib = (base_num & 0x07) == 4; // RSP or R12 requires SIB byte
    let (mod_bits, disp_bytes) = if disp == 0 && (base_num & 0x07) != 5 {
        // mod 00: [base] without displacement (except RBP/R13 which requires disp8 0)
        (0b00, vec![])
    } else if disp >= -128 && disp <= 127 {
        // mod 01: disp8
        (0b01, vec![disp as u8])
    } else {
        // mod 10: disp32
        (0b10, disp.to_le_bytes().to_vec())
    };

    let rm_bits = if needs_sib { 0b100 } else { base_num & 0x07 };
    let modrm = (mod_bits << 6) | ((modrm_reg & 0x07) << 3) | rm_bits;

    let mut tail = Vec::with_capacity(1 + if needs_sib { 1 } else { 0 } + disp_bytes.len());
    tail.push(modrm);
    if needs_sib {
        // SIB: scale=0 (00), index=RSP(100=none), base=RSP(100) -> 0x24
        tail.push(0x24);
    }
    tail.extend_from_slice(&disp_bytes);
    (rex, tail)
}

/// An item in an assembleable machine code sequence: either a label or an instruction/jump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineItem {
    Label(String),
    Inst(MachineInst),
    JmpLabel {
        target: String,
        provenance: Provenance,
    },
    JccLabel {
        cond: CondCode,
        target: String,
        provenance: Provenance,
    },
    CallLabel {
        target: String,
        provenance: Provenance,
    },
}

/// Assembles a sequence of machine items, resolving all labels and relative branch offsets.
pub fn assemble_program(items: &[MachineItem]) -> Result<Vec<u8>, String> {
    use std::collections::HashMap;

    // Pass 1: compute byte offsets of each item and record label positions
    let mut label_offsets = HashMap::new();
    let mut current_offset: usize = 0;

    for item in items {
        match item {
            MachineItem::Label(name) => {
                if label_offsets.insert(name.clone(), current_offset).is_some() {
                    return Err(format!("duplicate label: {name}"));
                }
            }
            MachineItem::Inst(inst) => {
                current_offset += inst.encode_bytes().len();
            }
            MachineItem::JmpLabel { .. } => {
                current_offset += 5; // 0xE9 + 4-byte displacement
            }
            MachineItem::JccLabel { .. } => {
                current_offset += 6; // 0x0F 0x8x + 4-byte displacement
            }
            MachineItem::CallLabel { .. } => {
                current_offset += 5; // 0xE8 + 4-byte displacement
            }
        }
    }

    // Pass 2: encode instructions and compute relative displacements
    let mut bytes = Vec::with_capacity(current_offset);

    for item in items {
        match item {
            MachineItem::Label(_) => {}
            MachineItem::Inst(inst) => {
                bytes.extend_from_slice(&inst.encode_bytes());
            }
            MachineItem::JmpLabel { target, provenance } => {
                let target_offset = label_offsets
                    .get(target)
                    .ok_or_else(|| format!("unresolved label: {target}"))?;
                let next_ip = bytes.len() + 5;
                let disp = (*target_offset as isize) - (next_ip as isize);
                let inst = MachineInst::JmpRel32 {
                    disp: disp as i32,
                    provenance: provenance.clone(),
                };
                bytes.extend_from_slice(&inst.encode_bytes());
            }
            MachineItem::JccLabel {
                cond,
                target,
                provenance,
            } => {
                let target_offset = label_offsets
                    .get(target)
                    .ok_or_else(|| format!("unresolved label: {target}"))?;
                let next_ip = bytes.len() + 6;
                let disp = (*target_offset as isize) - (next_ip as isize);
                let inst = MachineInst::JccRel32 {
                    cond: *cond,
                    disp: disp as i32,
                    provenance: provenance.clone(),
                };
                bytes.extend_from_slice(&inst.encode_bytes());
            }
            MachineItem::CallLabel { target, provenance } => {
                let target_offset = label_offsets
                    .get(target)
                    .ok_or_else(|| format!("unresolved label: {target}"))?;
                let next_ip = bytes.len() + 5;
                let disp = (*target_offset as isize) - (next_ip as isize);
                let inst = MachineInst::CallRel32 {
                    disp: disp as i32,
                    provenance: provenance.clone(),
                };
                bytes.extend_from_slice(&inst.encode_bytes());
            }
        }
    }

    Ok(bytes)
}

/// Target selection pass: lower a backend-neutral `MachineOp` into target-specific `MachineInst`s.
pub fn select_machine_primitive(op: MachineOp, fixnum_tag: u64) -> Vec<MachineInst> {
    match op {
        MachineOp::Rdtsc => {
            let prov = Provenance::new(
                None,
                "compiler target mechanism: rdtsc cycle counter normalized into tagged fixnum",
            );
            vec![
                MachineInst::Rdtsc {
                    provenance: prov.clone(),
                },
                MachineInst::ShlImm {
                    reg: X86Reg::Rdx,
                    imm: 32,
                    provenance: prov.clone(),
                },
                MachineInst::AluRegReg {
                    op: AluOp::Or,
                    dst: X86Reg::Rax,
                    src: X86Reg::Rdx,
                    provenance: prov.clone(),
                },
                MachineInst::MovImm64 {
                    dst: X86Reg::Rcx,
                    imm: 0x0FFFFFFFFFFFFFFF,
                    provenance: prov.clone(),
                },
                MachineInst::AluRegReg {
                    op: AluOp::And,
                    dst: X86Reg::Rax,
                    src: X86Reg::Rcx,
                    provenance: prov.clone(),
                },
                MachineInst::ShlImm {
                    reg: X86Reg::Rax,
                    imm: 3,
                    provenance: prov.clone(),
                },
                MachineInst::AluImm8 {
                    op: AluOp::Or,
                    dst: X86Reg::Rax,
                    imm: fixnum_tag as i8,
                    provenance: prov,
                },
            ]
        }
    }
}

/// Errors occurring during target selection for the vertical slice witness.
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

/// Target selection for the minimal vertical slice witness (#36):
/// Lowers an admitted fixnum arithmetic IR expression into executable `MachineItem`s.
///
/// Implements full standalone Linux execution:
/// 1. Evaluates arithmetic on tagged fixnums using canonical ABI tags.
/// 2. Writes the 8-byte little-endian tagged result word to stdout via `sys_write`.
/// 3. Exits cleanly via `sys_exit` with the unboxed fixnum exit code.
pub fn select_arithmetic_slice(program: &[Ir]) -> Result<Vec<MachineItem>, VerticalSliceError> {
    if program.is_empty() {
        return Err(VerticalSliceError::EmptyProgram);
    }
    let prov = Provenance::new(None, "vertical slice witness #36");
    let mut items = Vec::new();

    // Entry point: _start
    items.push(MachineItem::Label("_start".to_string()));

    // Align stack to 16-byte boundary per SysV AMD64 ABI
    items.push(MachineItem::Inst(MachineInst::AluImm8 {
        op: AluOp::And,
        dst: X86Reg::Rsp,
        imm: -16,
        provenance: prov.clone(),
    }));

    match &program[0] {
        Ir::Int(val) => {
            let tagged = wsm_os_target::encode_fixnum(*val)
                .ok_or(VerticalSliceError::FixnumOutOfRange(*val))?;
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
            let a = match &args[0] {
                Ir::Int(n) => *n,
                _ => {
                    return Err(VerticalSliceError::UnsupportedIrVariant(
                        "non-integer operand",
                    ));
                }
            };
            let b = match &args[1] {
                Ir::Int(n) => *n,
                _ => {
                    return Err(VerticalSliceError::UnsupportedIrVariant(
                        "non-integer operand",
                    ));
                }
            };

            let tagged_a =
                wsm_os_target::encode_fixnum(a).ok_or(VerticalSliceError::FixnumOutOfRange(a))?;
            let tagged_b =
                wsm_os_target::encode_fixnum(b).ok_or(VerticalSliceError::FixnumOutOfRange(b))?;

            // Load tagged A into %rcx, untag by shifting right 3
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

            // Load tagged B into %rdx, untag by shifting right 3
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

            // Perform arithmetic operation: %rcx = %rcx OP %rdx
            let alu_op = match op {
                PrimOp::Add => AluOp::Add,
                PrimOp::Sub => AluOp::Sub,
                _ => unreachable!(),
            };
            items.push(MachineItem::Inst(MachineInst::AluRegReg {
                op: alu_op,
                dst: X86Reg::Rcx,
                src: X86Reg::Rdx,
                provenance: prov.clone(),
            }));

            // Retag: %rcx = (%rcx << 3) | 3
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

            // Move result into %rax
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

    // Now emit output & exit mechanism:
    // 1. Push %rax (64-bit tagged word) onto stack
    items.push(MachineItem::Inst(MachineInst::PushReg {
        reg: X86Reg::Rax,
        provenance: prov.clone(),
    }));

    // 2. sys_write(stdout=1, buf=%rsp, count=8):
    // %rsi = %rsp (pointer to buffer on stack)
    items.push(MachineItem::Inst(MachineInst::MovRegReg {
        dst: X86Reg::Rsi,
        src: X86Reg::Rsp,
        provenance: prov.clone(),
    }));
    // %rdi = 1 (stdout)
    items.push(MachineItem::Inst(MachineInst::MovImm64 {
        dst: X86Reg::Rdi,
        imm: 1,
        provenance: prov.clone(),
    }));
    // %rdx = 8 (count)
    items.push(MachineItem::Inst(MachineInst::MovImm64 {
        dst: X86Reg::Rdx,
        imm: 8,
        provenance: prov.clone(),
    }));
    // %rax = 1 (SYS_write)
    items.push(MachineItem::Inst(MachineInst::MovImm64 {
        dst: X86Reg::Rax,
        imm: 1,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::Syscall {
        provenance: prov.clone(),
    }));

    // 3. Pop %rax back
    items.push(MachineItem::Inst(MachineInst::PopReg {
        reg: X86Reg::Rax,
        provenance: prov.clone(),
    }));

    // 4. Untag into %rdi for exit code: %rdi = %rax >> 3
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

    // 5. sys_exit(%rdi): %rax = 60
    items.push(MachineItem::Inst(MachineInst::MovImm64 {
        dst: X86Reg::Rax,
        imm: 60,
        provenance: prov.clone(),
    }));
    items.push(MachineItem::Inst(MachineInst::Syscall { provenance: prov }));

    Ok(items)
}

/// Project a sequence of `MachineItem`s into standard GNU assembler text.
pub fn items_to_gnu_asm(items: &[MachineItem]) -> String {
    let mut s = String::from(".text\n.globl _start\n.type _start, @function\n");
    for item in items {
        match item {
            MachineItem::Label(l) => s.push_str(&format!("{l}:\n")),
            MachineItem::Inst(inst) => s.push_str(&format!("    {}\n", inst.print_gnu_asm())),
            MachineItem::JmpLabel { target, .. } => s.push_str(&format!("    jmp {target}\n")),
            MachineItem::JccLabel { cond, target, .. } => {
                s.push_str(&format!("    j{} {target}\n", cond.mnemonic_suffix()))
            }
            MachineItem::CallLabel { target, .. } => s.push_str(&format!("    call {target}\n")),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn assemble_gnu_as(text: &str) -> Vec<u8> {
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("cml-inst-test-{}-{nonce}", std::process::id()));
        let s_path = base.with_extension("s");
        let o_path = base.with_extension("o");
        let bin_path = base.with_extension("bin");

        let mut file = std::fs::File::create(&s_path).expect("create temp asm file");
        writeln!(file, ".global _start\n_start:\n    {text}").expect("write asm text");
        file.flush().expect("flush asm text");

        let output = Command::new("as")
            .arg("--64")
            .arg(&s_path)
            .arg("-o")
            .arg(&o_path)
            .output()
            .expect("run GNU as");
        assert!(
            output.status.success(),
            "GNU as failed to assemble `{text}`: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let objcopy = Command::new("objcopy")
            .arg("-O")
            .arg("binary")
            .arg("--only-section=.text")
            .arg(&o_path)
            .arg(&bin_path)
            .status()
            .expect("run objcopy");
        assert!(objcopy.success(), "objcopy failed");

        let bytes = std::fs::read(&bin_path).expect("read binary bytes");
        let _ = std::fs::remove_file(s_path);
        let _ = std::fs::remove_file(o_path);
        let _ = std::fs::remove_file(bin_path);
        bytes
    }

    #[test]
    fn test_provenance_and_semantic_id() {
        let insts = select_machine_primitive(MachineOp::Rdtsc, 1);
        assert_eq!(insts.len(), 7);
        for inst in &insts {
            // Must be None: compiler-owned target mechanism, never claiming language semantic ID
            assert_eq!(inst.provenance().semantic_id, None);
        }
    }

    #[test]
    fn test_print_gnu_asm_fidelity() {
        let insts = select_machine_primitive(MachineOp::Rdtsc, 1);
        let asm_lines: Vec<String> = insts.iter().map(|i| i.print_gnu_asm()).collect();
        assert_eq!(
            asm_lines,
            vec![
                "rdtsc",
                "shlq $32, %rdx",
                "orq %rdx, %rax",
                "movabsq $0x0FFFFFFFFFFFFFFF, %rcx",
                "andq %rcx, %rax",
                "shlq $3, %rax",
                "orq $1, %rax",
            ]
        );
    }

    #[test]
    fn test_byte_encoding_exact_gnu_as_oracle() {
        let insts = select_machine_primitive(MachineOp::Rdtsc, 1);
        for inst in &insts {
            let asm_text = inst.print_gnu_asm();
            let oracle_bytes = assemble_gnu_as(&asm_text);
            let direct_bytes = inst.encode_bytes();
            assert_eq!(
                direct_bytes, oracle_bytes,
                "encoding mismatch for instruction `{asm_text}`: direct={direct_bytes:02X?}, oracle={oracle_bytes:02X?}"
            );
        }
    }

    #[test]
    fn test_alu_and_mov_encoding_oracle() {
        let prov = Provenance::new(None, "alu test");
        let test_cases = vec![
            MachineInst::AluRegReg {
                op: AluOp::Add,
                dst: X86Reg::Rax,
                src: X86Reg::Rbx,
                provenance: prov.clone(),
            },
            MachineInst::AluRegReg {
                op: AluOp::Sub,
                dst: X86Reg::Rdi,
                src: X86Reg::Rsi,
                provenance: prov.clone(),
            },
            MachineInst::AluRegReg {
                op: AluOp::Cmp,
                dst: X86Reg::Rax,
                src: X86Reg::Rbx,
                provenance: prov.clone(),
            },
            MachineInst::AluImm8 {
                op: AluOp::Sub,
                dst: X86Reg::Rsp,
                imm: 8,
                provenance: prov.clone(),
            },
            MachineInst::AluImm8 {
                op: AluOp::Add,
                dst: X86Reg::Rax,
                imm: 16,
                provenance: prov.clone(),
            },
            MachineInst::AluImm32 {
                op: AluOp::Add,
                dst: X86Reg::Rax,
                imm: 0x123456,
                provenance: prov.clone(),
            },
            MachineInst::AluImm32 {
                op: AluOp::Sub,
                dst: X86Reg::R8,
                imm: 1000,
                provenance: prov.clone(),
            },
            MachineInst::AluImm32 {
                op: AluOp::And,
                dst: X86Reg::Rbx,
                imm: 0x7FFFFFFF,
                provenance: prov.clone(),
            },
            MachineInst::Lea {
                dst: X86Reg::Rax,
                base: X86Reg::Rsp,
                disp: 16,
                provenance: prov.clone(),
            },
            MachineInst::Lea {
                dst: X86Reg::R12,
                base: X86Reg::Rbx,
                disp: 8,
                provenance: prov.clone(),
            },
            MachineInst::Lea {
                dst: X86Reg::Rdi,
                base: X86Reg::Rsi,
                disp: 0,
                provenance: prov.clone(),
            },
            MachineInst::MovRegReg {
                dst: X86Reg::Rbx,
                src: X86Reg::Rax,
                provenance: prov.clone(),
            },
            MachineInst::MovRegReg {
                dst: X86Reg::Rdi,
                src: X86Reg::R12,
                provenance: prov.clone(),
            },
            MachineInst::MovStore {
                base: X86Reg::Rsp,
                disp: -16,
                src: X86Reg::Rax,
                provenance: prov.clone(),
            },
            MachineInst::MovLoad {
                dst: X86Reg::Rax,
                base: X86Reg::Rsp,
                disp: -16,
                provenance: prov.clone(),
            },
            MachineInst::TestRegReg {
                reg1: X86Reg::Rax,
                reg2: X86Reg::Rax,
                provenance: prov.clone(),
            },
            MachineInst::SarImm {
                reg: X86Reg::Rcx,
                imm: 3,
                provenance: prov.clone(),
            },
            MachineInst::ShrImm {
                reg: X86Reg::Rdx,
                imm: 4,
                provenance: prov.clone(),
            },
            MachineInst::PushReg {
                reg: X86Reg::Rbx,
                provenance: prov.clone(),
            },
            MachineInst::PushReg {
                reg: X86Reg::R12,
                provenance: prov.clone(),
            },
            MachineInst::PopReg {
                reg: X86Reg::R12,
                provenance: prov.clone(),
            },
            MachineInst::PopReg {
                reg: X86Reg::Rbx,
                provenance: prov.clone(),
            },
            MachineInst::Syscall {
                provenance: prov.clone(),
            },
            MachineInst::Nop {
                provenance: prov.clone(),
            },
            MachineInst::Ret {
                provenance: prov.clone(),
            },
        ];

        for inst in test_cases {
            let asm_text = inst.print_gnu_asm();
            let oracle_bytes = assemble_gnu_as(&asm_text);
            let direct_bytes = inst.encode_bytes();
            assert_eq!(
                direct_bytes, oracle_bytes,
                "encoding mismatch for `{asm_text}`: direct={direct_bytes:02X?}, oracle={oracle_bytes:02X?}"
            );
        }
    }

    #[test]
    fn test_assemble_program_two_pass_labels() {
        let prov = Provenance::new(None, "assemble_program test");
        let items = vec![
            MachineItem::Inst(MachineInst::AluImm8 {
                op: AluOp::Cmp,
                dst: X86Reg::Rax,
                imm: 0,
                provenance: prov.clone(),
            }),
            MachineItem::JccLabel {
                cond: CondCode::Equal,
                target: "is_zero".to_string(),
                provenance: prov.clone(),
            },
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rax,
                imm: 1,
                provenance: prov.clone(),
            }),
            MachineItem::JmpLabel {
                target: "done".to_string(),
                provenance: prov.clone(),
            },
            MachineItem::Label("is_zero".to_string()),
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rax,
                imm: 2,
                provenance: prov.clone(),
            }),
            MachineItem::Label("done".to_string()),
            MachineItem::Inst(MachineInst::Ret { provenance: prov }),
        ];

        let bytes = assemble_program(&items).expect("assemble two-pass program");
        assert!(!bytes.is_empty());
        // Verify that bytes disassemble or match expected length
        // cmp: 4, jcc: 6, mov: 10, jmp: 5, mov: 10, ret: 1 = 36 bytes
        assert_eq!(bytes.len(), 36);
        // Verify relative displacement in Jcc (target is offset 25, jcc starts at 4, next_ip is 10 -> disp = 15 = 0x0F)
        assert_eq!(bytes[4..6], [0x0F, 0x84]);
        assert_eq!(i32::from_le_bytes(bytes[6..10].try_into().unwrap()), 15);
    }

    #[test]
    fn test_assemble_program_and_run_native_elf() {
        use crate::elf64::Elf64Executable;

        let prov = Provenance::new(None, "native elf test");
        // Compute (10 + 32) and exit with that code via Linux syscall
        let items = vec![
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rdi,
                imm: 10,
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::AluImm8 {
                op: AluOp::Add,
                dst: X86Reg::Rdi,
                imm: 32,
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rax,
                imm: 60, // sys_exit
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::Syscall { provenance: prov }),
        ];

        let code = assemble_program(&items).expect("assemble exit program");
        let elf = Elf64Executable::new(code);

        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("cml-native-prog-{nonce}"));

        elf.write_executable(&path).expect("write executable");
        let output = std::process::Command::new(&path)
            .output()
            .expect("run assembled ELF");
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            output.status.code(),
            Some(42),
            "Native assembled program must compute 10+32=42 and exit with 42"
        );
    }
}
