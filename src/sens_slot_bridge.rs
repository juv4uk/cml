//! Mechanical lowering from a verified SENS compiler call to SLOT-VM.
//!
//! Semantic admission has already happened in `sens_domain_bridge`. This
//! layer only materializes admitted literal data and applies the already
//! selected CML-private mechanism reference.

use crate::ir::{Ir, Quoted};
use crate::sens_domain_bridge::VerifiedDomainCall;
use crate::slot_vm::{Slot, SlotInstr, SlotProgram};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotBridgeError {
    Arity { expected: usize, actual: usize },
    ArgumentMustBeCanonicalLiteral,
    UnsupportedLiteral(&'static str),
    SlotSpaceExhausted,
}

impl fmt::Display for SlotBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, actual } => {
                write!(formatter, "verified SLOT bridge expects {expected} argument(s), got {actual}")
            }
            Self::ArgumentMustBeCanonicalLiteral => {
                write!(formatter, "first SLOT bridge accepts only an already-canonical literal value")
            }
            Self::UnsupportedLiteral(kind) => {
                write!(formatter, "literal kind is outside the first SLOT bridge slice: {kind}")
            }
            Self::SlotSpaceExhausted => write!(formatter, "SLOT bridge exhausted u16 slot space"),
        }
    }
}

impl std::error::Error for SlotBridgeError {}

struct Builder {
    instructions: Vec<SlotInstr>,
    next_slot: u16,
}

impl Builder {
    fn new() -> Self {
        Self {
            instructions: Vec::new(),
            next_slot: 0,
        }
    }

    fn alloc(&mut self) -> Result<Slot, SlotBridgeError> {
        if self.next_slot == u16::MAX {
            return Err(SlotBridgeError::SlotSpaceExhausted);
        }
        let slot = Slot::new(self.next_slot);
        self.next_slot += 1;
        Ok(slot)
    }

    fn materialize(&mut self, value: &Quoted) -> Result<Slot, SlotBridgeError> {
        match value {
            Quoted::Nil => {
                let dst = self.alloc()?;
                self.instructions.push(SlotInstr::LoadNil { dst });
                Ok(dst)
            }
            Quoted::Int(value) => {
                let dst = self.alloc()?;
                self.instructions.push(SlotInstr::LoadInt {
                    dst,
                    value: *value,
                });
                Ok(dst)
            }
            Quoted::List(items) => {
                let mut tail = self.materialize(&Quoted::Nil)?;
                for item in items.iter().rev() {
                    let head = self.materialize(item)?;
                    let dst = self.alloc()?;
                    self.instructions.push(SlotInstr::Cons { dst, head, tail });
                    tail = dst;
                }
                Ok(tail)
            }
            Quoted::DottedList(items, dotted_tail) => {
                let mut tail = self.materialize(dotted_tail)?;
                for item in items.iter().rev() {
                    let head = self.materialize(item)?;
                    let dst = self.alloc()?;
                    self.instructions.push(SlotInstr::Cons { dst, head, tail });
                    tail = dst;
                }
                Ok(tail)
            }
            Quoted::Float(_) => Err(SlotBridgeError::UnsupportedLiteral("float")),
            Quoted::Rational(_, _) => Err(SlotBridgeError::UnsupportedLiteral("rational")),
            Quoted::Sym { .. } => Err(SlotBridgeError::UnsupportedLiteral("symbol")),
            Quoted::Str(_) => Err(SlotBridgeError::UnsupportedLiteral("string")),
        }
    }
}

/// Lower the first verified one-argument selector call to deterministic SLOT-VM.
///
/// This function never inspects SENS identity, domain width, packed bits, or
/// human surface names. The target instruction comes only from the private
/// mechanism reference already attached by `verify_call`.
pub fn lower_verified_call_to_slot_program(
    call: &VerifiedDomainCall,
    source_case_id: impl Into<String>,
) -> Result<SlotProgram, SlotBridgeError> {
    let args = call.args();
    if args.len() != 1 {
        return Err(SlotBridgeError::Arity {
            expected: 1,
            actual: args.len(),
        });
    }

    let Ir::Quote(argument) = &args[0] else {
        return Err(SlotBridgeError::ArgumentMustBeCanonicalLiteral);
    };

    let mut builder = Builder::new();
    let pair = builder.materialize(argument)?;
    let dst = builder.alloc()?;
    builder
        .instructions
        .push(call.mechanism_ref().slot_instruction(dst, pair));
    builder.instructions.push(SlotInstr::Return { src: dst });

    Ok(SlotProgram {
        source_case_id: Some(source_case_id.into()),
        slot_count: builder.next_slot,
        instructions: builder.instructions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler_mechanism::CompilerMechanismRef;
    use crate::sens_domain_bridge::{
        MechanismStatus, SemanticRequest, SemanticStatus, pinned_authority, verify_call,
    };

    fn verified(
        identity: sens::DomainIdentity,
        role: sens::CompilerExecutionRole,
        argument: Quoted,
    ) -> VerifiedDomainCall {
        verify_call(
            SemanticRequest {
                identity,
                execution_role: role,
                law_ref: "language-contract.lisp:d3-foundation".into(),
                proof_ref: "contracts/bija3-l1-l5-ratification.lisp".into(),
                semantic_status: SemanticStatus::Current,
                mechanism_status: MechanismStatus::Admitted,
                provenance: pinned_authority().unwrap(),
            },
            vec![Ir::Quote(argument)],
        )
        .unwrap()
    }

    fn d3(raw: u8) -> sens::DomainIdentity {
        sens::DomainIdentity::D3(sens::Bija3::from_word(
            sens::Bit3::new(raw).unwrap(),
        ))
    }

    #[test]
    fn verified_head_role_materializes_data_then_emits_selected_slot_mechanism() {
        let call = verified(
            d3(0b100),
            sens::CompilerExecutionRole::SelectorHead,
            Quoted::List(vec![Quoted::List(vec![Quoted::Nil])]),
        );
        assert_eq!(call.mechanism_ref(), CompilerMechanismRef::SlotVmCar);

        let program = lower_verified_call_to_slot_program(&call, "case-head").unwrap();
        assert_eq!(program.source_case_id.as_deref(), Some("case-head"));
        assert!(matches!(
            program.instructions.get(program.instructions.len() - 2),
            Some(SlotInstr::Car { .. })
        ));
        assert!(matches!(
            program.instructions.last(),
            Some(SlotInstr::Return { .. })
        ));
    }

    #[test]
    fn nonliteral_argument_fails_closed_without_target_execution() {
        let call = verify_call(
            SemanticRequest {
                identity: d3(0b011),
                execution_role: sens::CompilerExecutionRole::SelectorTail,
                law_ref: "language-contract.lisp:d3-foundation".into(),
                proof_ref: "contracts/bija3-l1-l5-ratification.lisp".into(),
                semantic_status: SemanticStatus::Current,
                mechanism_status: MechanismStatus::Admitted,
                provenance: pinned_authority().unwrap(),
            },
            vec![Ir::Var("X".into())],
        )
        .unwrap();

        assert_eq!(
            lower_verified_call_to_slot_program(&call, "nonliteral").unwrap_err(),
            SlotBridgeError::ArgumentMustBeCanonicalLiteral
        );
    }
}
