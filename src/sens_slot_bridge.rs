//! Mechanical lowering from verified SENS compiler roles to SLOT-VM.
//!
//! Semantic admission is owned by SENS and verified in `sens_domain_bridge`.
//! This layer recursively lowers only the bounded canonical forms admitted by
//! the current compiler slice. It never decodes domain bits or surface names.

use crate::ast::Expr as CExpr;
use crate::compiler_mechanism::CompilerMechanismRef;
use crate::ir::{Ir, Quoted};
use crate::sens_domain_bridge::{
    BridgeError, MechanismStatus, SemanticRequest, SemanticStatus, VerifiedDomainCall,
    pinned_authority, verify_request,
};
use crate::slot_vm::{Slot, SlotInstr, SlotProgram};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotBridgeError {
    Arity { expected: usize, actual: usize },
    ArgumentMustBeCanonicalLiteral,
    UnsupportedLiteral(&'static str),
    UnsupportedCanonicalForm(&'static str),
    UnsupportedVerifiedMechanism,
    Verification(BridgeError),
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
            Self::UnsupportedCanonicalForm(kind) => {
                write!(formatter, "canonical form is outside the current exact-domain compiler slice: {kind}")
            }
            Self::UnsupportedVerifiedMechanism => {
                write!(formatter, "verified mechanism is not valid for this SLOT lowering shape")
            }
            Self::Verification(error) => write!(formatter, "SENS request verification failed: {error}"),
            Self::SlotSpaceExhausted => write!(formatter, "SLOT bridge exhausted u16 slot space"),
        }
    }
}

impl std::error::Error for SlotBridgeError {}

impl From<BridgeError> for SlotBridgeError {
    fn from(error: BridgeError) -> Self {
        Self::Verification(error)
    }
}

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

    fn load_nil(&mut self) -> Result<Slot, SlotBridgeError> {
        let dst = self.alloc()?;
        self.instructions.push(SlotInstr::LoadNil { dst });
        Ok(dst)
    }

    fn materialize(&mut self, value: &Quoted) -> Result<Slot, SlotBridgeError> {
        match value {
            Quoted::Nil => self.load_nil(),
            Quoted::Int(value) => {
                let dst = self.alloc()?;
                self.instructions.push(SlotInstr::LoadInt {
                    dst,
                    value: *value,
                });
                Ok(dst)
            }
            Quoted::List(items) => {
                let mut tail = self.load_nil()?;
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

    fn compile_canonical_expr(&mut self, expr: &CExpr) -> Result<Slot, SlotBridgeError> {
        match expr {
            // Canonical SENS reader represents exact D3 structural empty 000
            // as the empty list value. It is data, never a callable role.
            CExpr::List(items) if items.is_empty() => self.load_nil(),
            CExpr::List(items) => self.compile_canonical_call(items),
            CExpr::DomainIdentity(_) => {
                Err(SlotBridgeError::UnsupportedCanonicalForm("bare domain identity"))
            }
            CExpr::DottedList(_, _) => {
                Err(SlotBridgeError::UnsupportedCanonicalForm("dotted source form"))
            }
            CExpr::Sid(_) => Err(SlotBridgeError::UnsupportedCanonicalForm("legacy Sid8")),
            CExpr::Integer(_) => Err(SlotBridgeError::UnsupportedCanonicalForm("integer")),
            CExpr::Rational(_, _) => Err(SlotBridgeError::UnsupportedCanonicalForm("rational")),
            CExpr::Symbol(_) => Err(SlotBridgeError::UnsupportedCanonicalForm("surface symbol")),
            CExpr::String(_) => Err(SlotBridgeError::UnsupportedCanonicalForm("string")),
            CExpr::NumericBuffer(_) => {
                Err(SlotBridgeError::UnsupportedCanonicalForm("numeric buffer"))
            }
        }
    }

    fn compile_canonical_call(&mut self, items: &[CExpr]) -> Result<Slot, SlotBridgeError> {
        let Some((head, args)) = items.split_first() else {
            return self.load_nil();
        };
        let CExpr::DomainIdentity(identity) = head else {
            return Err(SlotBridgeError::UnsupportedCanonicalForm(
                "call head is not exact DomainIdentity",
            ));
        };

        let core = identity
            .core_operation()
            .ok_or(SlotBridgeError::UnsupportedCanonicalForm(
                "domain identity has no callable Core projection",
            ))?;
        let role = sens::compiler_execution_role_from_sens(core)
            .map_err(|_| {
                SlotBridgeError::Verification(BridgeError::LanguageRoleDerivationFailed)
            })?
            .ok_or(SlotBridgeError::UnsupportedCanonicalForm(
                "domain identity has no admitted compiler execution role",
            ))?;

        let verified = verify_request(SemanticRequest {
            identity: *identity,
            execution_role: role,
            law_ref: "language-contract.lisp:d3-foundation".into(),
            proof_ref: "contracts/bija3-l1-l5-ratification.lisp".into(),
            semantic_status: SemanticStatus::Current,
            mechanism_status: MechanismStatus::Admitted,
            provenance: pinned_authority()?,
        })?;

        match verified.mechanism_ref() {
            CompilerMechanismRef::SlotVmCar | CompilerMechanismRef::SlotVmCdr => {
                if args.len() != 1 {
                    return Err(SlotBridgeError::Arity {
                        expected: 1,
                        actual: args.len(),
                    });
                }
                let pair = self.compile_canonical_expr(&args[0])?;
                let dst = self.alloc()?;
                let instruction = verified
                    .mechanism_ref()
                    .selector_instruction(dst, pair)
                    .ok_or(SlotBridgeError::UnsupportedVerifiedMechanism)?;
                self.instructions.push(instruction);
                Ok(dst)
            }
            CompilerMechanismRef::SlotVmCons => {
                if args.len() != 2 {
                    return Err(SlotBridgeError::Arity {
                        expected: 2,
                        actual: args.len(),
                    });
                }
                let head = self.compile_canonical_expr(&args[0])?;
                let tail = self.compile_canonical_expr(&args[1])?;
                let dst = self.alloc()?;
                let instruction = verified
                    .mechanism_ref()
                    .pair_instruction(dst, head, tail)
                    .ok_or(SlotBridgeError::UnsupportedVerifiedMechanism)?;
                self.instructions.push(instruction);
                Ok(dst)
            }
        }
    }
}

/// Compatibility path from #588 for a verified one-argument selector call.
///
/// It remains available for differential/bootstrap consumers, but new canonical
/// source compilation should use `lower_canonical_expr_to_slot_program`.
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
    let instruction = call
        .mechanism_ref()
        .selector_instruction(dst, pair)
        .ok_or(SlotBridgeError::UnsupportedVerifiedMechanism)?;
    builder.instructions.push(instruction);
    builder.instructions.push(SlotInstr::Return { src: dst });

    Ok(SlotProgram {
        source_case_id: Some(source_case_id.into()),
        slot_count: builder.next_slot,
        instructions: builder.instructions,
    })
}

/// Compile the bounded canonical D3 constructor/selector source slice directly
/// to SLOT-VM without evaluating the operand through SENS.
///
/// Every callable node is verified through the SENS execution-role API before
/// CML selects/emits its private target mechanism. Structural empty is data.
pub fn lower_canonical_expr_to_slot_program(
    expr: &CExpr,
    source_case_id: impl Into<String>,
) -> Result<SlotProgram, SlotBridgeError> {
    let mut builder = Builder::new();
    let result = builder.compile_canonical_expr(expr)?;
    builder.instructions.push(SlotInstr::Return { src: result });

    Ok(SlotProgram {
        source_case_id: Some(source_case_id.into()),
        slot_count: builder.next_slot,
        instructions: builder.instructions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sens_domain_bridge::{
        MechanismStatus, SemanticRequest, SemanticStatus, pinned_authority, verify_call,
    };
    use crate::slot_vm::execute;

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

    #[test]
    fn canonical_cons_tree_compiles_without_evaluator_operand_materialization() {
        let parsed = crate::parser::parse_canonical_binary(
            "10 100 00 10 111 00 10 111 00 000 00 000 01 00 000 01 01",
        )
        .unwrap();
        let [expr] = parsed.as_slice() else {
            panic!("expected one canonical source expression");
        };

        let program = lower_canonical_expr_to_slot_program(expr, "full-source").unwrap();
        assert!(program
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, SlotInstr::Cons { .. })));
        assert!(program
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, SlotInstr::Car { .. })));

        let result = execute(&program).expect("full-source SLOT program must execute");
        assert_eq!(result.value.to_string(), "(())");
    }
}
