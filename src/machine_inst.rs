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

    /// Bitwise OR two 64-bit registers: `orq %src, %dst`.
    OrRegReg {
        dst: X86Reg,
        src: X86Reg,
        provenance: Provenance,
    },

    /// Bitwise OR 64-bit register with immediate 8-bit value (sign-extended to 64): `orq $imm, %dst`.
    OrImm8 {
        dst: X86Reg,
        imm: u8,
        provenance: Provenance,
    },

    /// Bitwise AND two 64-bit registers: `andq %src, %dst`.
    AndRegReg {
        dst: X86Reg,
        src: X86Reg,
        provenance: Provenance,
    },

    /// Move 64-bit immediate into register: `movabsq $imm, %dst`.
    MovImm64 {
        dst: X86Reg,
        imm: u64,
        provenance: Provenance,
    },

    /// Near return to calling procedure: `ret`.
    Ret { provenance: Provenance },
}

impl MachineInst {
    /// Returns the instruction's provenance metadata.
    pub fn provenance(&self) -> &Provenance {
        match self {
            Self::Rdtsc { provenance }
            | Self::ShlImm { provenance, .. }
            | Self::OrRegReg { provenance, .. }
            | Self::OrImm8 { provenance, .. }
            | Self::AndRegReg { provenance, .. }
            | Self::MovImm64 { provenance, .. }
            | Self::Ret { provenance } => provenance,
        }
    }

    /// Projection A: Render instruction as standard GNU assembly text.
    pub fn print_gnu_asm(&self) -> String {
        match self {
            Self::Rdtsc { .. } => "rdtsc".to_string(),
            Self::ShlImm { reg, imm, .. } => format!("shlq ${imm}, {}", reg.name()),
            Self::OrRegReg { dst, src, .. } => format!("orq {}, {}", src.name(), dst.name()),
            Self::OrImm8 { dst, imm, .. } => format!("orq ${imm}, {}", dst.name()),
            Self::AndRegReg { dst, src, .. } => format!("andq {}, {}", src.name(), dst.name()),
            Self::MovImm64 { dst, imm, .. } => format!("movabsq $0x{imm:016X}, {}", dst.name()),
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

            Self::OrRegReg { dst, src, .. } => {
                // REX.W=1, REX.R=src bit 3, REX.B=dst bit 3
                let rex = 0x48
                    | if src.is_extended() { 0x04 } else { 0x00 }
                    | if dst.is_extended() { 0x01 } else { 0x00 };
                // 09 /r: OR r/m64, r64
                let opcode = 0x09;
                let modrm = (0b11 << 6) | ((src.number() & 0x07) << 3) | (dst.number() & 0x07);
                vec![rex, opcode, modrm]
            }

            Self::OrImm8 { dst, imm, .. } => {
                let rex = 0x48 | if dst.is_extended() { 0x01 } else { 0x00 };
                // 83 /1 ib: OR r/m64, imm8
                let opcode = 0x83;
                let modrm = (0b11 << 6) | (0b001 << 3) | (dst.number() & 0x07);
                vec![rex, opcode, modrm, *imm]
            }

            Self::AndRegReg { dst, src, .. } => {
                let rex = 0x48
                    | if src.is_extended() { 0x04 } else { 0x00 }
                    | if dst.is_extended() { 0x01 } else { 0x00 };
                // 21 /r: AND r/m64, r64
                let opcode = 0x21;
                let modrm = (0b11 << 6) | ((src.number() & 0x07) << 3) | (dst.number() & 0x07);
                vec![rex, opcode, modrm]
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

            Self::Ret { .. } => vec![0xC3],
        }
    }
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
                MachineInst::OrRegReg {
                    dst: X86Reg::Rax,
                    src: X86Reg::Rdx,
                    provenance: prov.clone(),
                },
                MachineInst::MovImm64 {
                    dst: X86Reg::Rcx,
                    imm: 0x0FFFFFFFFFFFFFFF,
                    provenance: prov.clone(),
                },
                MachineInst::AndRegReg {
                    dst: X86Reg::Rax,
                    src: X86Reg::Rcx,
                    provenance: prov.clone(),
                },
                MachineInst::ShlImm {
                    reg: X86Reg::Rax,
                    imm: 3,
                    provenance: prov.clone(),
                },
                MachineInst::OrImm8 {
                    dst: X86Reg::Rax,
                    imm: fixnum_tag as u8,
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
}
