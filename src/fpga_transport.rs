//! Versioned host-side protocol for executing a preassembled fpga-lisp image.
//!
//! This models the real ISA-1.0 UART bootloader and post-HALT monitor bytes.
//! Serial-port ownership is delegated to a transport implementation so WSL
//! COM bridges, native serial libraries, simulation, and future PCIe can share
//! one CML boundary.

pub const FPGA_JOB_PROTOCOL_VERSION: u16 = 1;
pub const MAX_PROGRAM_WORDS: usize = 4095;
pub const TAG_FIXNUM: u8 = 0;
pub const MONITOR_REG: u8 = 0x01;
pub const MONITOR_ERROR: u8 = 0x04;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FpgaJobV1 {
    pub program_words: Vec<u32>,
    pub result_register: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FpgaResultV1 {
    pub result_word: u32,
    pub error_flag: bool,
    pub error_pc: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FpgaProtocolError {
    EmptyProgram,
    ProgramTooLong(usize),
    InvalidResultRegister(u8),
    InvalidErrorStatusLength(usize),
    HardwareError { pc: u16 },
    UnexpectedTag { expected: u8, actual: u8 },
    Transport(String),
}

impl FpgaJobV1 {
    pub fn validate(&self) -> Result<(), FpgaProtocolError> {
        if self.program_words.is_empty() {
            return Err(FpgaProtocolError::EmptyProgram);
        }
        if self.program_words.len() > MAX_PROGRAM_WORDS {
            return Err(FpgaProtocolError::ProgramTooLong(self.program_words.len()));
        }
        if self.result_register > 15 {
            return Err(FpgaProtocolError::InvalidResultRegister(
                self.result_register,
            ));
        }
        Ok(())
    }

    /// Exact bytes consumed by fpga-lisp ISA-1.0's UART bootloader:
    /// little-endian u16 instruction count followed by little-endian words.
    pub fn bootloader_frame(&self) -> Result<Vec<u8>, FpgaProtocolError> {
        self.validate()?;
        let mut frame = Vec::with_capacity(2 + self.program_words.len() * 4);
        frame.extend_from_slice(&(self.program_words.len() as u16).to_le_bytes());
        for word in &self.program_words {
            frame.extend_from_slice(&word.to_le_bytes());
        }
        Ok(frame)
    }

    pub fn result_query(&self) -> Result<[u8; 2], FpgaProtocolError> {
        self.validate()?;
        Ok([MONITOR_REG, self.result_register])
    }
}

impl FpgaResultV1 {
    pub fn from_monitor_words(result_word: u32, error_status: u32) -> Self {
        Self {
            result_word,
            error_flag: ((error_status >> 12) & 1) != 0,
            error_pc: (error_status & 0x0fff) as u16,
        }
    }

    pub fn checked_fixnum(self) -> Result<i32, FpgaProtocolError> {
        if self.error_flag {
            return Err(FpgaProtocolError::HardwareError { pc: self.error_pc });
        }
        let tag = (self.result_word >> 28) as u8;
        if tag != TAG_FIXNUM {
            return Err(FpgaProtocolError::UnexpectedTag {
                expected: TAG_FIXNUM,
                actual: tag,
            });
        }
        let payload = self.result_word & 0x0fff_ffff;
        Ok(if payload & 0x0800_0000 != 0 {
            (payload | 0xf000_0000) as i32
        } else {
            payload as i32
        })
    }
}

/// Physical transport lifecycle: reset/admit the bootloader frame, wait for
/// HALT, issue monitor register/error queries, and return their decoded words.
pub trait FpgaTransport {
    fn execute(&mut self, job: &FpgaJobV1) -> Result<FpgaResultV1, FpgaProtocolError>;
}

pub struct FpgaJobExecutor<T> {
    transport: T,
}

impl<T> FpgaJobExecutor<T>
where
    T: FpgaTransport,
{
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn execute_fixnum(&mut self, job: &FpgaJobV1) -> Result<i32, FpgaProtocolError> {
        job.validate()?;
        self.transport.execute(job)?.checked_fixnum()
    }
}
