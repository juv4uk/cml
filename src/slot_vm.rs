//! Minimal deterministic register/slot execution target for cml#454.
//!
//! This module is deliberately a *mechanism* layer. It does not map SENS
//! domain identities, surface names, or legacy SID spellings to operations.
//! A semantics-authoritative upstream bridge/compiler pass must select an
//! admitted mechanism before constructing a `SlotProgram`.
//!
//! The first slice keeps the instruction set intentionally small: immutable
//! integer/list data, pair construction/selectors, and an explicit return.
//! Unsupported language/IR forms are therefore absent rather than silently
//! interpreted here.

use std::fmt;

/// A virtual register/slot in a `SlotProgram`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Slot(u16);

impl Slot {
    pub const fn new(index: u16) -> Self {
        Self(index)
    }

    pub const fn index(self) -> u16 {
        self.0
    }
}

/// Values admitted by the first SLOT-VM mechanism slice.
///
/// Pairs are immutable values. This is an implementation representation, not a
/// language-level statement about allocation, identity, or mutability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotValue {
    Int(i64),
    Nil,
    Pair(Box<SlotValue>, Box<SlotValue>),
}

impl SlotValue {
    fn kind_name(&self) -> &'static str {
        match self {
            Self::Int(_) => "integer",
            Self::Nil => "nil",
            Self::Pair(_, _) => "pair",
        }
    }
}

impl fmt::Display for SlotValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(value) => write!(formatter, "{value}"),
            Self::Nil => write!(formatter, "()"),
            Self::Pair(head, tail) => {
                write!(formatter, "(")?;
                write!(formatter, "{head}")?;
                let mut cursor = tail.as_ref();
                loop {
                    match cursor {
                        Self::Nil => {
                            write!(formatter, ")")?;
                            break;
                        }
                        Self::Pair(next_head, next_tail) => {
                            write!(formatter, " {next_head}")?;
                            cursor = next_tail.as_ref();
                        }
                        other => {
                            write!(formatter, " . {other})")?;
                            break;
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

/// First deliberately small register/slot instruction surface.
///
/// These are already-selected physical/compiler mechanisms. In particular,
/// `Car` and `Cdr` are not an authority table for any SENS domain bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotInstr {
    LoadInt { dst: Slot, value: i64 },
    LoadNil { dst: Slot },
    Cons { dst: Slot, head: Slot, tail: Slot },
    Car { dst: Slot, pair: Slot },
    Cdr { dst: Slot, pair: Slot },
    Return { src: Slot },
}

/// Deterministic SLOT-VM program artifact.
///
/// `source_case_id` is opaque provenance copied from the caller. The VM never
/// interprets it; retaining it prevents an execution result from becoming
/// detached from the upstream conformance case that produced the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotProgram {
    pub source_case_id: Option<String>,
    pub slot_count: u16,
    pub instructions: Vec<SlotInstr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotExecution {
    pub source_case_id: Option<String>,
    pub value: SlotValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotVmError {
    SlotOutOfRange {
        pc: usize,
        slot: Slot,
        slot_count: u16,
    },
    UninitializedSlot {
        pc: usize,
        slot: Slot,
    },
    Type {
        pc: usize,
        op: &'static str,
        expected: &'static str,
        found: &'static str,
    },
    MissingReturn,
    ReturnNotLast {
        pc: usize,
    },
    InvalidMagic,
    InvalidProvenanceTag {
        tag: u8,
    },
    Truncated {
        offset: usize,
        needed: usize,
    },
    LengthOverflow {
        field: &'static str,
        value: u64,
    },
    InvalidUtf8,
    UnknownOpcode {
        pc: usize,
        opcode: u8,
    },
    TrailingBytes {
        offset: usize,
        remaining: usize,
    },
}

impl fmt::Display for SlotVmError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlotOutOfRange {
                pc,
                slot,
                slot_count,
            } => write!(
                formatter,
                "slot {} out of range at pc {pc} (slot_count={slot_count})",
                slot.index()
            ),
            Self::UninitializedSlot { pc, slot } => {
                write!(
                    formatter,
                    "read of uninitialized slot {} at pc {pc}",
                    slot.index()
                )
            }
            Self::Type {
                pc,
                op,
                expected,
                found,
            } => write!(
                formatter,
                "{op} type error at pc {pc}: expected {expected}, found {found}"
            ),
            Self::MissingReturn => write!(formatter, "slot program has no return"),
            Self::ReturnNotLast { pc } => {
                write!(formatter, "return at pc {pc} is not the final instruction")
            }
            Self::InvalidMagic => write!(formatter, "invalid CMLSLOT1 artifact magic/version"),
            Self::InvalidProvenanceTag { tag } => {
                write!(formatter, "invalid CMLSLOT1 provenance tag {tag}")
            }
            Self::Truncated { offset, needed } => write!(
                formatter,
                "truncated CMLSLOT1 artifact at byte {offset}: need {needed} more byte(s)"
            ),
            Self::LengthOverflow { field, value } => {
                write!(formatter, "CMLSLOT1 {field} length {value} does not fit this host")
            }
            Self::InvalidUtf8 => write!(formatter, "CMLSLOT1 provenance is not valid UTF-8"),
            Self::UnknownOpcode { pc, opcode } => {
                write!(formatter, "unknown CMLSLOT1 opcode 0x{opcode:02x} at instruction {pc}")
            }
            Self::TrailingBytes { offset, remaining } => write!(
                formatter,
                "CMLSLOT1 artifact has {remaining} trailing byte(s) at byte {offset}"
            ),
        }
    }
}

impl std::error::Error for SlotVmError {}

impl SlotProgram {
    /// Validate structural well-formedness before execution/serialization.
    pub fn validate(&self) -> Result<(), SlotVmError> {
        let mut saw_return = false;

        for (pc, instruction) in self.instructions.iter().enumerate() {
            match instruction {
                SlotInstr::LoadInt { dst, .. } | SlotInstr::LoadNil { dst } => {
                    self.ensure_slot(pc, *dst)?;
                }
                SlotInstr::Cons { dst, head, tail } => {
                    self.ensure_slot(pc, *dst)?;
                    self.ensure_slot(pc, *head)?;
                    self.ensure_slot(pc, *tail)?;
                }
                SlotInstr::Car { dst, pair } | SlotInstr::Cdr { dst, pair } => {
                    self.ensure_slot(pc, *dst)?;
                    self.ensure_slot(pc, *pair)?;
                }
                SlotInstr::Return { src } => {
                    self.ensure_slot(pc, *src)?;
                    saw_return = true;
                    if pc + 1 != self.instructions.len() {
                        return Err(SlotVmError::ReturnNotLast { pc });
                    }
                }
            }
        }

        if !saw_return {
            return Err(SlotVmError::MissingReturn);
        }

        Ok(())
    }

    fn ensure_slot(&self, pc: usize, slot: Slot) -> Result<(), SlotVmError> {
        if slot.index() >= self.slot_count {
            return Err(SlotVmError::SlotOutOfRange {
                pc,
                slot,
                slot_count: self.slot_count,
            });
        }
        Ok(())
    }

    /// Stable binary representation for the current v1 slot artifact.
    ///
    /// Encoding is explicit-width and little-endian:
    /// `CMLSLOT1 | slot_count:u16 | provenance | instruction_count:u64 | ops`.
    /// It is an artifact format only; it does not assign language meaning.
    pub fn encode_v1(&self) -> Result<Vec<u8>, SlotVmError> {
        self.validate()?;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"CMLSLOT1");
        bytes.extend_from_slice(&self.slot_count.to_le_bytes());

        match &self.source_case_id {
            Some(case_id) => {
                bytes.push(1);
                bytes.extend_from_slice(&(case_id.len() as u64).to_le_bytes());
                bytes.extend_from_slice(case_id.as_bytes());
            }
            None => bytes.push(0),
        }

        bytes.extend_from_slice(&(self.instructions.len() as u64).to_le_bytes());

        for instruction in &self.instructions {
            match instruction {
                SlotInstr::LoadInt { dst, value } => {
                    bytes.push(0x01);
                    push_slot(&mut bytes, *dst);
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                SlotInstr::LoadNil { dst } => {
                    bytes.push(0x02);
                    push_slot(&mut bytes, *dst);
                }
                SlotInstr::Cons { dst, head, tail } => {
                    bytes.push(0x10);
                    push_slot(&mut bytes, *dst);
                    push_slot(&mut bytes, *head);
                    push_slot(&mut bytes, *tail);
                }
                SlotInstr::Car { dst, pair } => {
                    bytes.push(0x11);
                    push_slot(&mut bytes, *dst);
                    push_slot(&mut bytes, *pair);
                }
                SlotInstr::Cdr { dst, pair } => {
                    bytes.push(0x12);
                    push_slot(&mut bytes, *dst);
                    push_slot(&mut bytes, *pair);
                }
                SlotInstr::Return { src } => {
                    bytes.push(0xff);
                    push_slot(&mut bytes, *src);
                }
            }
        }

        Ok(bytes)
    }

    /// Decode and structurally validate one exact v1 SLOT-VM artifact.
    ///
    /// The decoder is intentionally strict: the magic/version, provenance
    /// tag, UTF-8, opcode set, declared instruction count, slot bounds, return
    /// shape, and end-of-input must all agree. There is no version fallback or
    /// ignored trailer.
    pub fn decode_v1(bytes: &[u8]) -> Result<Self, SlotVmError> {
        let mut decoder = Decoder::new(bytes);

        if decoder.take_exact::<8>()? != *b"CMLSLOT1" {
            return Err(SlotVmError::InvalidMagic);
        }

        let slot_count = decoder.take_u16()?;
        let source_case_id = match decoder.take_u8()? {
            0 => None,
            1 => {
                let length_u64 = decoder.take_u64()?;
                let length =
                    usize::try_from(length_u64).map_err(|_| SlotVmError::LengthOverflow {
                        field: "provenance",
                        value: length_u64,
                    })?;
                let raw = decoder.take_bytes(length)?;
                Some(
                    std::str::from_utf8(raw)
                        .map_err(|_| SlotVmError::InvalidUtf8)?
                        .to_string(),
                )
            }
            tag => return Err(SlotVmError::InvalidProvenanceTag { tag }),
        };

        let instruction_count_u64 = decoder.take_u64()?;
        let instruction_count =
            usize::try_from(instruction_count_u64).map_err(|_| SlotVmError::LengthOverflow {
                field: "instruction-count",
                value: instruction_count_u64,
            })?;

        let mut instructions = Vec::new();
        for pc in 0..instruction_count {
            let opcode = decoder.take_u8()?;
            let instruction = match opcode {
                0x01 => SlotInstr::LoadInt {
                    dst: decoder.take_slot()?,
                    value: decoder.take_i64()?,
                },
                0x02 => SlotInstr::LoadNil {
                    dst: decoder.take_slot()?,
                },
                0x10 => SlotInstr::Cons {
                    dst: decoder.take_slot()?,
                    head: decoder.take_slot()?,
                    tail: decoder.take_slot()?,
                },
                0x11 => SlotInstr::Car {
                    dst: decoder.take_slot()?,
                    pair: decoder.take_slot()?,
                },
                0x12 => SlotInstr::Cdr {
                    dst: decoder.take_slot()?,
                    pair: decoder.take_slot()?,
                },
                0xff => SlotInstr::Return {
                    src: decoder.take_slot()?,
                },
                opcode => return Err(SlotVmError::UnknownOpcode { pc, opcode }),
            };
            instructions.push(instruction);
        }

        if decoder.remaining() != 0 {
            return Err(SlotVmError::TrailingBytes {
                offset: decoder.offset(),
                remaining: decoder.remaining(),
            });
        }

        let program = Self {
            source_case_id,
            slot_count,
            instructions,
        };
        program.validate()?;
        Ok(program)
    }
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn offset(&self) -> usize {
        self.offset
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take_bytes(&mut self, length: usize) -> Result<&'a [u8], SlotVmError> {
        if self.remaining() < length {
            return Err(SlotVmError::Truncated {
                offset: self.offset,
                needed: length - self.remaining(),
            });
        }
        let start = self.offset;
        self.offset += length;
        Ok(&self.bytes[start..self.offset])
    }

    fn take_exact<const N: usize>(&mut self) -> Result<[u8; N], SlotVmError> {
        let raw = self.take_bytes(N)?;
        let mut result = [0_u8; N];
        result.copy_from_slice(raw);
        Ok(result)
    }

    fn take_u8(&mut self) -> Result<u8, SlotVmError> {
        Ok(self.take_exact::<1>()?[0])
    }

    fn take_u16(&mut self) -> Result<u16, SlotVmError> {
        Ok(u16::from_le_bytes(self.take_exact::<2>()?))
    }

    fn take_u64(&mut self) -> Result<u64, SlotVmError> {
        Ok(u64::from_le_bytes(self.take_exact::<8>()?))
    }

    fn take_i64(&mut self) -> Result<i64, SlotVmError> {
        Ok(i64::from_le_bytes(self.take_exact::<8>()?))
    }

    fn take_slot(&mut self) -> Result<Slot, SlotVmError> {
        Ok(Slot::new(self.take_u16()?))
    }
}

fn push_slot(bytes: &mut Vec<u8>, slot: Slot) {
    bytes.extend_from_slice(&slot.index().to_le_bytes());
}

fn read_slot(slots: &[Option<SlotValue>], pc: usize, slot: Slot) -> Result<SlotValue, SlotVmError> {
    slots[usize::from(slot.index())]
        .clone()
        .ok_or(SlotVmError::UninitializedSlot { pc, slot })
}

fn write_slot(slots: &mut [Option<SlotValue>], slot: Slot, value: SlotValue) {
    slots[usize::from(slot.index())] = Some(value);
}

/// Execute one validated slot program.
///
/// There is deliberately no implicit fallback: wrong value shape,
/// uninitialized input, malformed slot index, or missing terminator returns a
/// named error.
pub fn execute(program: &SlotProgram) -> Result<SlotExecution, SlotVmError> {
    program.validate()?;

    let mut slots = vec![None; usize::from(program.slot_count)];

    for (pc, instruction) in program.instructions.iter().enumerate() {
        match instruction {
            SlotInstr::LoadInt { dst, value } => {
                write_slot(&mut slots, *dst, SlotValue::Int(*value));
            }
            SlotInstr::LoadNil { dst } => {
                write_slot(&mut slots, *dst, SlotValue::Nil);
            }
            SlotInstr::Cons { dst, head, tail } => {
                let head = read_slot(&slots, pc, *head)?;
                let tail = read_slot(&slots, pc, *tail)?;
                write_slot(
                    &mut slots,
                    *dst,
                    SlotValue::Pair(Box::new(head), Box::new(tail)),
                );
            }
            SlotInstr::Car { dst, pair } => {
                let pair_value = read_slot(&slots, pc, *pair)?;
                let result = match pair_value {
                    SlotValue::Pair(head, _) => *head,
                    other => {
                        return Err(SlotVmError::Type {
                            pc,
                            op: "car",
                            expected: "pair",
                            found: other.kind_name(),
                        });
                    }
                };
                write_slot(&mut slots, *dst, result);
            }
            SlotInstr::Cdr { dst, pair } => {
                let pair_value = read_slot(&slots, pc, *pair)?;
                let result = match pair_value {
                    SlotValue::Pair(_, tail) => *tail,
                    other => {
                        return Err(SlotVmError::Type {
                            pc,
                            op: "cdr",
                            expected: "pair",
                            found: other.kind_name(),
                        });
                    }
                };
                write_slot(&mut slots, *dst, result);
            }
            SlotInstr::Return { src } => {
                let value = read_slot(&slots, pc, *src)?;
                return Ok(SlotExecution {
                    source_case_id: program.source_case_id.clone(),
                    value,
                });
            }
        }
    }

    Err(SlotVmError::MissingReturn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d3_fixture_prefix(case_id: &str) -> SlotProgram {
        // s0 = ()
        // s1 = (())      = (cons () ())
        // s2 = ((()))    = (cons (()) ())
        //
        // The following selector instruction is intentionally supplied by
        // the test/caller. SLOT-VM itself does not map SENS bits to CAR/CDR.
        SlotProgram {
            source_case_id: Some(case_id.to_string()),
            slot_count: 4,
            instructions: vec![
                SlotInstr::LoadNil { dst: Slot::new(0) },
                SlotInstr::Cons {
                    dst: Slot::new(1),
                    head: Slot::new(0),
                    tail: Slot::new(0),
                },
                SlotInstr::Cons {
                    dst: Slot::new(2),
                    head: Slot::new(1),
                    tail: Slot::new(0),
                },
            ],
        }
    }

    #[test]
    fn car_mechanism_matches_current_d3_fixture_shape_and_preserves_case_id() {
        let mut program = d3_fixture_prefix("sens-d3-car");
        program.instructions.extend([
            SlotInstr::Car {
                dst: Slot::new(3),
                pair: Slot::new(2),
            },
            SlotInstr::Return { src: Slot::new(3) },
        ]);

        let result = execute(&program).expect("mechanical CAR witness must execute");
        assert_eq!(result.source_case_id.as_deref(), Some("sens-d3-car"));
        assert_eq!(result.value.to_string(), "(())");
    }

    #[test]
    fn cdr_mechanism_matches_current_d3_fixture_shape_and_preserves_case_id() {
        let mut program = d3_fixture_prefix("sens-d3-cdr");
        program.instructions.extend([
            SlotInstr::Cdr {
                dst: Slot::new(3),
                pair: Slot::new(2),
            },
            SlotInstr::Return { src: Slot::new(3) },
        ]);

        let result = execute(&program).expect("mechanical CDR witness must execute");
        assert_eq!(result.source_case_id.as_deref(), Some("sens-d3-cdr"));
        assert_eq!(result.value.to_string(), "()");
    }

    #[test]
    fn selector_on_empty_fails_closed_as_type_error() {
        let program = SlotProgram {
            source_case_id: Some("sens-d3-car-empty".into()),
            slot_count: 2,
            instructions: vec![
                SlotInstr::LoadNil { dst: Slot::new(0) },
                SlotInstr::Car {
                    dst: Slot::new(1),
                    pair: Slot::new(0),
                },
                SlotInstr::Return { src: Slot::new(1) },
            ],
        };

        assert_eq!(
            execute(&program),
            Err(SlotVmError::Type {
                pc: 1,
                op: "car",
                expected: "pair",
                found: "nil",
            })
        );
    }

    #[test]
    fn uninitialized_reads_fail_closed() {
        let program = SlotProgram {
            source_case_id: None,
            slot_count: 2,
            instructions: vec![
                SlotInstr::Car {
                    dst: Slot::new(1),
                    pair: Slot::new(0),
                },
                SlotInstr::Return { src: Slot::new(1) },
            ],
        };

        assert_eq!(
            execute(&program),
            Err(SlotVmError::UninitializedSlot {
                pc: 0,
                slot: Slot::new(0),
            })
        );
    }

    #[test]
    fn out_of_range_slots_are_rejected_before_execution() {
        let program = SlotProgram {
            source_case_id: None,
            slot_count: 1,
            instructions: vec![
                SlotInstr::LoadNil { dst: Slot::new(1) },
                SlotInstr::Return { src: Slot::new(0) },
            ],
        };

        assert_eq!(
            program.validate(),
            Err(SlotVmError::SlotOutOfRange {
                pc: 0,
                slot: Slot::new(1),
                slot_count: 1,
            })
        );
    }

    #[test]
    fn encoding_is_deterministic_explicit_width_and_provenance_sensitive() {
        let mut program = d3_fixture_prefix("case-a");
        program.instructions.extend([
            SlotInstr::Cdr {
                dst: Slot::new(3),
                pair: Slot::new(2),
            },
            SlotInstr::Return { src: Slot::new(3) },
        ]);

        let first = program.encode_v1().expect("valid program");
        let second = program.encode_v1().expect("valid program");
        assert_eq!(first, second);
        assert_eq!(&first[..8], b"CMLSLOT1");
        assert_eq!(&first[8..10], &4_u16.to_le_bytes());

        let mut other_case = program.clone();
        other_case.source_case_id = Some("case-b".into());
        assert_ne!(
            first,
            other_case
                .encode_v1()
                .expect("same program, new provenance")
        );
    }


    #[test]
    fn encoding_round_trips_through_strict_v1_decoder() {
        let mut program = d3_fixture_prefix("випадок-d3");
        program.instructions.extend([
            SlotInstr::Car {
                dst: Slot::new(3),
                pair: Slot::new(2),
            },
            SlotInstr::Return { src: Slot::new(3) },
        ]);

        let bytes = program.encode_v1().expect("valid program");
        let decoded = SlotProgram::decode_v1(&bytes).expect("encoded v1 must decode");
        assert_eq!(decoded, program);
        assert_eq!(
            execute(&decoded).expect("decoded program executes").value.to_string(),
            "(())"
        );
    }

    #[test]
    fn decoder_rejects_magic_opcode_truncation_and_trailing_bytes() {
        let program = SlotProgram {
            source_case_id: None,
            slot_count: 1,
            instructions: vec![
                SlotInstr::LoadNil { dst: Slot::new(0) },
                SlotInstr::Return { src: Slot::new(0) },
            ],
        };
        let bytes = program.encode_v1().expect("valid program");

        let mut bad_magic = bytes.clone();
        bad_magic[0] ^= 1;
        assert_eq!(
            SlotProgram::decode_v1(&bad_magic),
            Err(SlotVmError::InvalidMagic)
        );

        // No provenance: 8 magic + 2 slot_count + 1 tag + 8 instruction_count.
        let mut bad_opcode = bytes.clone();
        bad_opcode[19] = 0x7f;
        assert_eq!(
            SlotProgram::decode_v1(&bad_opcode),
            Err(SlotVmError::UnknownOpcode {
                pc: 0,
                opcode: 0x7f,
            })
        );

        let truncated = &bytes[..bytes.len() - 1];
        assert!(matches!(
            SlotProgram::decode_v1(truncated),
            Err(SlotVmError::Truncated { .. })
        ));

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            SlotProgram::decode_v1(&trailing),
            Err(SlotVmError::TrailingBytes { remaining: 1, .. })
        ));
    }

    #[test]
    fn decoder_rejects_invalid_provenance_tag_and_utf8() {
        let program = SlotProgram {
            source_case_id: Some("x".into()),
            slot_count: 1,
            instructions: vec![
                SlotInstr::LoadNil { dst: Slot::new(0) },
                SlotInstr::Return { src: Slot::new(0) },
            ],
        };
        let bytes = program.encode_v1().expect("valid program");

        let mut bad_tag = bytes.clone();
        bad_tag[10] = 2;
        assert_eq!(
            SlotProgram::decode_v1(&bad_tag),
            Err(SlotVmError::InvalidProvenanceTag { tag: 2 })
        );

        // 8 magic + 2 slot_count + 1 tag + 8 provenance length.
        let mut bad_utf8 = bytes.clone();
        bad_utf8[19] = 0xff;
        assert_eq!(
            SlotProgram::decode_v1(&bad_utf8),
            Err(SlotVmError::InvalidUtf8)
        );
    }

    #[test]
    fn return_must_be_the_final_instruction() {
        let program = SlotProgram {
            source_case_id: None,
            slot_count: 1,
            instructions: vec![
                SlotInstr::LoadNil { dst: Slot::new(0) },
                SlotInstr::Return { src: Slot::new(0) },
                SlotInstr::LoadNil { dst: Slot::new(0) },
            ],
        };

        assert_eq!(
            program.validate(),
            Err(SlotVmError::ReturnNotLast { pc: 1 })
        );
    }
}