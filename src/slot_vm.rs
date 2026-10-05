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