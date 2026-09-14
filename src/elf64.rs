//! Minimal standalone ELF64 executable synthesizer for x86-64 Linux.
//!
//! Allows CML to emit native, self-contained ELF executables directly from
//! machine instruction bytes without external linkers (`ld`) or C runtimes.

use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// ELF Identification constants
pub const ELFMAG: [u8; 4] = [0x7F, b'E', b'L', b'F'];
pub const ELFCLASS64: u8 = 2;
pub const ELFDATA2LSB: u8 = 1;
pub const EV_CURRENT: u8 = 1;
pub const ELFOSABI_NONE: u8 = 0;

/// ELF Header constants
pub const ET_EXEC: u16 = 2;
pub const EM_X86_64: u16 = 62;

/// Program Header constants
pub const PT_LOAD: u32 = 1;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

/// Default virtual address base for static executable loading (4 MiB).
pub const DEFAULT_LOAD_VADDR: u64 = 0x400000;

/// Size of 64-bit ELF header in bytes.
pub const ELF64_EHDR_SIZE: usize = 64;

/// Size of 64-bit Program Header in bytes.
pub const ELF64_PHDR_SIZE: usize = 56;

/// Total size of file headers before machine code.
pub const TOTAL_HEADERS_SIZE: usize = ELF64_EHDR_SIZE + ELF64_PHDR_SIZE;

/// A self-contained 64-bit static executable image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elf64Executable {
    pub load_vaddr: u64,
    pub entry_vaddr: u64,
    pub code: Vec<u8>,
}

impl Elf64Executable {
    /// Create a new executable where code starts immediately after the headers.
    pub fn new(code: Vec<u8>) -> Self {
        let load_vaddr = DEFAULT_LOAD_VADDR;
        let entry_vaddr = load_vaddr + TOTAL_HEADERS_SIZE as u64;
        Self {
            load_vaddr,
            entry_vaddr,
            code,
        }
    }

    /// Serialize the entire ELF executable into raw bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let total_file_size = TOTAL_HEADERS_SIZE + self.code.len();
        let mut bytes = Vec::with_capacity(total_file_size);

        // 1. ELF Header (64 bytes)
        // e_ident (16 bytes)
        bytes.extend_from_slice(&ELFMAG);
        bytes.push(ELFCLASS64);
        bytes.push(ELFDATA2LSB);
        bytes.push(EV_CURRENT);
        bytes.push(ELFOSABI_NONE);
        bytes.extend_from_slice(&[0u8; 8]); // EI_ABIVERSION + padding

        // e_type (2 bytes)
        bytes.extend_from_slice(&ET_EXEC.to_le_bytes());
        // e_machine (2 bytes)
        bytes.extend_from_slice(&EM_X86_64.to_le_bytes());
        // e_version (4 bytes)
        bytes.extend_from_slice(&(EV_CURRENT as u32).to_le_bytes());
        // e_entry (8 bytes)
        bytes.extend_from_slice(&self.entry_vaddr.to_le_bytes());
        // e_phoff (8 bytes) - Program headers immediately follow ELF header (offset 64)
        bytes.extend_from_slice(&(ELF64_EHDR_SIZE as u64).to_le_bytes());
        // e_shoff (8 bytes) - No section headers required for runtime execution
        bytes.extend_from_slice(&0u64.to_le_bytes());
        // e_flags (4 bytes)
        bytes.extend_from_slice(&0u32.to_le_bytes());
        // e_ehsize (2 bytes)
        bytes.extend_from_slice(&(ELF64_EHDR_SIZE as u16).to_le_bytes());
        // e_phentsize (2 bytes)
        bytes.extend_from_slice(&(ELF64_PHDR_SIZE as u16).to_le_bytes());
        // e_phnum (2 bytes) - Exactly 1 PT_LOAD segment
        bytes.extend_from_slice(&1u16.to_le_bytes());
        // e_shentsize (2 bytes)
        bytes.extend_from_slice(&(64u16).to_le_bytes());
        // e_shnum (2 bytes)
        bytes.extend_from_slice(&0u16.to_le_bytes());
        // e_shstrndx (2 bytes)
        bytes.extend_from_slice(&0u16.to_le_bytes());

        debug_assert_eq!(bytes.len(), ELF64_EHDR_SIZE, "ELF header must be 64 bytes");

        // 2. Program Header (56 bytes)
        let flags = PF_R | PF_X; // Read + Execute
        // p_type (4 bytes)
        bytes.extend_from_slice(&PT_LOAD.to_le_bytes());
        // p_flags (4 bytes)
        bytes.extend_from_slice(&flags.to_le_bytes());
        // p_offset (8 bytes)
        bytes.extend_from_slice(&0u64.to_le_bytes());
        // p_vaddr (8 bytes)
        bytes.extend_from_slice(&self.load_vaddr.to_le_bytes());
        // p_paddr (8 bytes)
        bytes.extend_from_slice(&self.load_vaddr.to_le_bytes());
        // p_filesz (8 bytes)
        bytes.extend_from_slice(&(total_file_size as u64).to_le_bytes());
        // p_memsz (8 bytes)
        bytes.extend_from_slice(&(total_file_size as u64).to_le_bytes());
        // p_align (8 bytes)
        bytes.extend_from_slice(&0x1000u64.to_le_bytes());

        debug_assert_eq!(
            bytes.len(),
            TOTAL_HEADERS_SIZE,
            "Headers must be 120 bytes in total"
        );

        // 3. Machine code bytes
        bytes.extend_from_slice(&self.code);

        bytes
    }

    /// Write executable to path and grant execution permissions (`0o755`).
    pub fn write_executable<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let bytes = self.to_bytes();
        let mut file = File::create(&path)?;
        file.write_all(&bytes)?;
        file.flush()?;

        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn test_elf_header_structure() {
        let code = vec![0x90]; // NOP
        let elf = Elf64Executable::new(code);
        let bytes = elf.to_bytes();

        assert_eq!(bytes.len(), TOTAL_HEADERS_SIZE + 1);
        assert_eq!(&bytes[0..4], &ELFMAG);
        assert_eq!(bytes[4], ELFCLASS64);
        assert_eq!(bytes[5], ELFDATA2LSB);
        assert_eq!(bytes[18..20], EM_X86_64.to_le_bytes());
        assert_eq!(bytes[TOTAL_HEADERS_SIZE], 0x90);
    }

    #[test]
    fn test_native_standalone_elf_execution_without_linker() {
        // x86-64 machine code:
        // mov $60, %eax   (sys_exit)
        // mov $37, %edi   (exit status 37)
        // syscall
        let code = vec![
            0xB8, 0x3C, 0x00, 0x00, 0x00, // mov $60, %eax
            0xBF, 0x25, 0x00, 0x00, 0x00, // mov $37, %edi
            0x0F, 0x05, // syscall
        ];

        let elf = Elf64Executable::new(code);
        let nonce = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(format!("cml-elf-test-{nonce}"));

        elf.write_executable(&path).expect("write executable");

        let output = Command::new(&path).output().expect("run standalone ELF");
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            output.status.code(),
            Some(37),
            "Standalone ELF must execute directly and return exit code 37"
        );
    }
}
