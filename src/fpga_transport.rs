//! Versioned host-side protocol for executing a preassembled fpga-lisp image.
//!
//! This models the real ISA-1.1 UART bootloader and post-HALT monitor bytes.
//! Serial-port ownership is delegated to a transport implementation so WSL
//! COM bridges, native serial libraries, simulation, and future PCIe can share
//! one CML boundary.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::ir::BufferLiteral;

pub const FPGA_JOB_PROTOCOL_VERSION: u16 = 1;
pub const MAX_PROGRAM_WORDS: usize = 4095;
pub const MAX_REGISTER_INPUTS: usize = 16;
pub const TAG_FIXNUM: u8 = 0;
pub const FIXNUM_MIN: i32 = -(1 << 27);
pub const FIXNUM_MAX: i32 = (1 << 27) - 1;
pub const MONITOR_REG: u8 = 0x01;
pub const MONITOR_ERROR: u8 = 0x04;
const BRIDGE_REQUEST_MAGIC: [u8; 4] = *b"CMLJ";
const BRIDGE_RESPONSE_MAGIC: [u8; 4] = *b"CMLR";
const BRIDGE_RESPONSE_LEN: usize = 14;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FpgaJobV1 {
    pub program_words: Vec<u32>,
    pub register_inputs: Vec<FpgaRegisterInput>,
    pub result_register: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FpgaRegisterInput {
    pub register: u8,
    pub tagged_word: u32,
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
    TooManyRegisterInputs(usize),
    InvalidInputRegister(u8),
    DuplicateInputRegister(u8),
    InvalidErrorStatusLength(usize),
    HardwareError { pc: u16 },
    UnexpectedTag { expected: u8, actual: u8 },
    UnsupportedInputBuffer,
    EmptyInputBuffer,
    InputValueOutOfRange { index: usize, value: i32 },
    RegisterRange { first: u8, count: usize },
    Transport(String),
}

/// Materialize a contiguous i32 buffer as ISA 1.1 tagged FIXNUM inputs.
///
/// This is deliberately an explicit host-staged boundary: F32 buffers and
/// values outside the fpga-lisp FIXNUM range are rejected rather than
/// silently rounded or truncated. The caller must still attach the returned
/// inputs to a graph node with an explicit data dependency.
pub fn encode_i32_buffer_as_register_inputs(
    buffer: &BufferLiteral,
    first_register: u8,
) -> Result<Vec<FpgaRegisterInput>, FpgaProtocolError> {
    let values = match buffer {
        BufferLiteral::I32(values) => values,
        BufferLiteral::F32(_) => return Err(FpgaProtocolError::UnsupportedInputBuffer),
    };
    if values.is_empty() {
        return Err(FpgaProtocolError::EmptyInputBuffer);
    }
    if values.len() > MAX_REGISTER_INPUTS || usize::from(first_register) + values.len() > 16 {
        return Err(FpgaProtocolError::RegisterRange {
            first: first_register,
            count: values.len(),
        });
    }
    values
        .iter()
        .enumerate()
        .map(|(index, &value)| {
            if !(FIXNUM_MIN..=FIXNUM_MAX).contains(&value) {
                return Err(FpgaProtocolError::InputValueOutOfRange { index, value });
            }
            Ok(FpgaRegisterInput {
                register: first_register + index as u8,
                tagged_word: (i64::from(value) & 0x0fff_ffff) as u32,
            })
        })
        .collect()
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
        if self.register_inputs.len() > MAX_REGISTER_INPUTS {
            return Err(FpgaProtocolError::TooManyRegisterInputs(
                self.register_inputs.len(),
            ));
        }
        let mut seen = [false; 16];
        for input in &self.register_inputs {
            if input.register > 15 {
                return Err(FpgaProtocolError::InvalidInputRegister(input.register));
            }
            if seen[input.register as usize] {
                return Err(FpgaProtocolError::DuplicateInputRegister(input.register));
            }
            seen[input.register as usize] = true;
        }
        Ok(())
    }

    /// Exact bytes consumed by fpga-lisp ISA-1.1's UART bootloader. Jobs with
    /// no register inputs retain the ISA-1.0 frame byte-for-byte. Extended
    /// jobs set length bit 15 and insert validated register/tagged-word records.
    pub fn bootloader_frame(&self) -> Result<Vec<u8>, FpgaProtocolError> {
        self.validate()?;
        let extra = if self.register_inputs.is_empty() {
            0
        } else {
            1 + self.register_inputs.len() * 5
        };
        let mut frame = Vec::with_capacity(2 + extra + self.program_words.len() * 4);
        let mut header = self.program_words.len() as u16;
        if !self.register_inputs.is_empty() {
            header |= 0x8000;
        }
        frame.extend_from_slice(&header.to_le_bytes());
        if !self.register_inputs.is_empty() {
            frame.push(self.register_inputs.len() as u8);
            for input in &self.register_inputs {
                frame.push(input.register);
                frame.extend_from_slice(&input.tagged_word.to_le_bytes());
            }
        }
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
        let word = self.checked_word()?;
        let tag = (word >> 28) as u8;
        if tag != TAG_FIXNUM {
            return Err(FpgaProtocolError::UnexpectedTag {
                expected: TAG_FIXNUM,
                actual: tag,
            });
        }
        let payload = word & 0x0fff_ffff;
        Ok(if payload & 0x0800_0000 != 0 {
            (payload | 0xf000_0000) as i32
        } else {
            payload as i32
        })
    }

    pub fn checked_word(self) -> Result<u32, FpgaProtocolError> {
        if self.error_flag {
            Err(FpgaProtocolError::HardwareError { pc: self.error_pc })
        } else {
            Ok(self.result_word)
        }
    }
}

/// Physical transport lifecycle: reset/admit the bootloader frame, wait for
/// HALT, issue monitor register/error queries, and return their decoded words.
pub trait FpgaTransport {
    fn execute(&mut self, job: &FpgaJobV1) -> Result<FpgaResultV1, FpgaProtocolError>;
}

/// Executes a physical FPGA job through a small host-native bridge process.
///
/// This is the WSL/Windows boundary: CML owns the versioned request and
/// response, while the configured process owns COM-port access, reset timing,
/// serial framing, and exact reads. No shell is involved and program bytes are
/// written on stdin, so the 4095-word ISA limit is not constrained by command
/// line length or temporary-file naming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandFpgaTransport {
    program: PathBuf,
    args: Vec<String>,
}

impl CommandFpgaTransport {
    pub fn new(program: impl Into<PathBuf>, args: impl IntoIterator<Item = String>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().collect(),
        }
    }
}

impl FpgaTransport for CommandFpgaTransport {
    fn execute(&mut self, job: &FpgaJobV1) -> Result<FpgaResultV1, FpgaProtocolError> {
        let request = bridge_request(job)?;
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                FpgaProtocolError::Transport(format!(
                    "failed to start FPGA bridge {}: {error}",
                    self.program.display()
                ))
            })?;

        child
            .stdin
            .take()
            .ok_or_else(|| FpgaProtocolError::Transport("FPGA bridge stdin unavailable".into()))?
            .write_all(&request)
            .map_err(|error| {
                FpgaProtocolError::Transport(format!("failed to write FPGA bridge job: {error}"))
            })?;

        let output = child.wait_with_output().map_err(|error| {
            FpgaProtocolError::Transport(format!("failed to wait for FPGA bridge: {error}"))
        })?;
        if !output.status.success() {
            return Err(FpgaProtocolError::Transport(format!(
                "FPGA bridge exited with {}",
                output.status
            )));
        }
        parse_bridge_response(&output.stdout)
    }
}

fn bridge_request(job: &FpgaJobV1) -> Result<Vec<u8>, FpgaProtocolError> {
    let frame = job.bootloader_frame()?;
    let mut request = Vec::with_capacity(8 + frame.len());
    request.extend_from_slice(&BRIDGE_REQUEST_MAGIC);
    request.extend_from_slice(&FPGA_JOB_PROTOCOL_VERSION.to_le_bytes());
    request.push(job.result_register);
    request.push(0);
    request.extend_from_slice(&frame);
    Ok(request)
}

fn parse_bridge_response(bytes: &[u8]) -> Result<FpgaResultV1, FpgaProtocolError> {
    if bytes.len() != BRIDGE_RESPONSE_LEN {
        return Err(FpgaProtocolError::Transport(format!(
            "FPGA bridge response has {} bytes, expected {BRIDGE_RESPONSE_LEN}",
            bytes.len()
        )));
    }
    if bytes[..4] != BRIDGE_RESPONSE_MAGIC {
        return Err(FpgaProtocolError::Transport(
            "FPGA bridge response magic mismatch".into(),
        ));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != FPGA_JOB_PROTOCOL_VERSION {
        return Err(FpgaProtocolError::Transport(format!(
            "FPGA bridge protocol version {version} is unsupported"
        )));
    }
    let result_word = u32::from_le_bytes(bytes[6..10].try_into().expect("fixed response slice"));
    let error_status = u32::from_le_bytes(bytes[10..14].try_into().expect("fixed response slice"));
    Ok(FpgaResultV1::from_monitor_words(result_word, error_status))
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

    pub fn execute_word(&mut self, job: &FpgaJobV1) -> Result<u32, FpgaProtocolError> {
        job.validate()?;
        self.transport.execute(job)?.checked_word()
    }
}
