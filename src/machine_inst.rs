//! `MachineInst`: structured machine instruction layer between backend-neutral
//! compiler IR (`Ir::MachinePrim`) and physical target projections (GNU assembly text
//! or direct machine bytes).
//!
//! # Architecture
//!
//! ```text
//! my-lisp semantic identity (e.g. read-cycle-counter, semantic ID 1153)
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

use crate::ir::MachineOp;

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
            | Self::AluRegReg { provenance, .. }
            | Self::AluImm8 { provenance, .. }
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
            Self::AluRegReg { op, dst, src, .. } => {
                format!("{} {}, {}", op.name(), src.name(), dst.name())
            }
            Self::AluImm8 { op, dst, imm, .. } => {
                format!("{} ${imm}, {}", op.name(), dst.name())
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

/// Target selection pass: lower a backend-neutral `MachineOp` into target-specific `MachineInst`s.
pub fn select_machine_primitive(op: MachineOp, fixnum_tag: u64) -> Vec<MachineInst> {
    match op {
        MachineOp::Rdtsc => {
            let prov = Provenance::new(
                Some("1153"),
                "rdtsc cycle counter normalized into tagged fixnum",
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

        let status = Command::new("as")
            .arg("--64")
            .arg(&s_path)
            .arg("-o")
            .arg(&o_path)
            .status()
            .expect("run GNU as");
        assert!(status.success(), "GNU as failed to assemble: {text}");

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
            assert_eq!(inst.provenance().semantic_id, Some("1153"));
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
            MachineInst::Nop {
                provenance: prov.clone(),
            },
            MachineInst::Ret { provenance: prov },
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
}
