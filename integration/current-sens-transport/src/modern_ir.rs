//! Candidate-only CML AST / IR boundary. Read and lower with SENS's canonical
//! D2 reader + SENS-owned exact compiler law; never resurrect byte-opcode SID.
//!
//! This is a strictly bounded compiler subset. Unsupported constructs fail
//! closed: this is not a replacement for the historical CML production backend.
use sens::{CompilerLoweringRole, CoreDomainIdentity, DomainIdentity};
use sens::syntax::{Expr, ExprKind};

use crate::VerifiedPhysicalProgram;

// The same Rust SOURCE defines the modern root CML AST/IR and candidate types.
#[path = "../../../src/current_exact_model.rs"]
mod shared_current_exact_model;
pub use shared_current_exact_model::{
    CmlMechanism, ExactQuotedData, ModernAst, ModernIr, VerifiedProgramIr, VerifiedRole,
};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/sens_compiler_candidates.rs"));
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModernLowerError {
    InvalidCanonicalSource,
    NoExecutableRoot,
    NonCallableHead,
    UnsupportedLiteral,
    UnsupportedForm,
    InvalidArity,
    InvalidConditionClause,
    StructuralDepthExceeded,
    UnverifiedCompilerRole,
    InvalidCompilerAuthority,
    InvalidGeneratedCorpus,
}

fn verified_role(identity: CoreDomainIdentity) -> Result<VerifiedRole, ModernLowerError> {
    // No role, proof or semantics are inferred from width / bits / name.
    let record = sens::compiler_semantic_input_from_sens(identity)
        .map_err(|_| ModernLowerError::UnverifiedCompilerRole)?
        .ok_or(ModernLowerError::UnverifiedCompilerRole)?;
    if record.identity != identity || record.semantic_status != "current"
        || record.authority_ref.is_empty() || record.proof_ref.is_empty()
        || record.authority_path != "language-contract.lisp"
        || record.language_contract_version.is_empty()
        || record.authority_sha256.len() != 64
    {
        return Err(ModernLowerError::InvalidCompilerAuthority);
    }
    let pinned_authority = sens::sha256_source(
        include_bytes!("../../../external/sens/language-contract.lisp"),
    ).iter().map(|b| format!("{b:02x}")).collect::<String>();
    if record.authority_sha256 != pinned_authority {
        return Err(ModernLowerError::InvalidCompilerAuthority);
    }
    Ok(VerifiedRole {
        identity, role: record.lowering_role,
        law_ref: record.authority_ref, proof_ref: record.proof_ref,
        authority_path: record.authority_path,
        authority_sha256: record.authority_sha256,
        contract_version: record.language_contract_version,
    })
}

/// Mechanical candidate identities are generated from the SENS-owned D3/D4
/// corpus, never from CML's legacy numeric table; every semantic role is
/// independently computed using SENS's *executable* compiler law.
pub fn compiler_registry() -> Result<Vec<VerifiedRole>, ModernLowerError> {
    let entries = generated::COMPILER_CANDIDATE_COORDINATES;
    if entries.len() != 9 { return Err(ModernLowerError::InvalidGeneratedCorpus); }
    let mut registry = Vec::with_capacity(9);
    for &(fixture, domain, bits) in entries {
        let raw = u8::from_str_radix(bits, 2)
            .map_err(|_| ModernLowerError::InvalidGeneratedCorpus)?;
        let identity = match domain {
            "D3" if bits.len() == 3 =>
                CoreDomainIdentity::D3(sens::Bija3::from_word(
                    sens::Bit3::new(raw).ok_or(ModernLowerError::InvalidGeneratedCorpus)?,
                )),
            "D4" if bits.len() == 4 =>
                CoreDomainIdentity::D4(sens::CoreD4::from_word(
                    sens::Bit4::new(raw).ok_or(ModernLowerError::InvalidGeneratedCorpus)?,
                )),
            _ => return Err(ModernLowerError::InvalidGeneratedCorpus),
        };
        if fixture != format!("nucleus-{}-{bits}", domain.to_ascii_lowercase()) {
            return Err(ModernLowerError::InvalidGeneratedCorpus);
        }
        let candidate = verified_role(identity)?;
        if registry.iter().any(|row: &VerifiedRole| row.identity == identity) {
            return Err(ModernLowerError::InvalidGeneratedCorpus);
        }
        registry.push(candidate);
    }
    if registry.iter().any(|r| r.authority_sha256 != registry[0].authority_sha256) {
        return Err(ModernLowerError::InvalidCompilerAuthority);
    }
    Ok(registry)
}

fn quoted_data(source: &Expr, depth: usize) -> Result<ExactQuotedData, ModernLowerError> {
    if depth > 128 { return Err(ModernLowerError::StructuralDepthExceeded); }
    match &source.kind {
        ExprKind::List(items) if items.is_empty() => Ok(ExactQuotedData::EmptyList),
        ExprKind::List(items) => Ok(ExactQuotedData::ProperList(items.iter()
            .map(|x| quoted_data(x, depth + 1))
            .collect::<Result<Vec<_>, _>>()?)),
        ExprKind::DomainIdentity(id) => Ok(ExactQuotedData::DomainWord(*id)),
        ExprKind::Pair(a, b) => Ok(ExactQuotedData::DottedPair(
            Box::new(quoted_data(a, depth + 1)?),
            Box::new(quoted_data(b, depth + 1)?),
        )),
        _ => Err(ModernLowerError::UnsupportedLiteral),
    }
}

fn exact_call(source: &Expr, depth: usize) -> Result<ModernAst, ModernLowerError> {
    if depth > 128 { return Err(ModernLowerError::StructuralDepthExceeded); }
    match &source.kind {
        ExprKind::List(items) if items.is_empty() =>
            Ok(ModernAst::Literal(ExactQuotedData::EmptyList)),
        ExprKind::DomainIdentity(DomainIdentity::D1(_)) =>
            Ok(ModernAst::Literal(quoted_data(source, depth)?)),
        ExprKind::DomainCall(identity, args) => {
            let binding = verified_role(*identity)?;
            match binding.role {
                CompilerLoweringRole::QuoteForm => {
                    if args.len() != 1 { return Err(ModernLowerError::InvalidArity); }
                    Ok(ModernAst::Quote {
                        binding,
                        data: quoted_data(&args[0], depth + 1)?,
                    })
                }
                CompilerLoweringRole::CondForm => {
                    let mut branches = Vec::with_capacity(args.len());
                    for clause in args.iter() {
                        let ExprKind::List(parts) = &clause.kind else {
                            return Err(ModernLowerError::InvalidConditionClause);
                        };
                        if parts.len() != 2 { return Err(ModernLowerError::InvalidConditionClause); }
                        branches.push((exact_call(&parts[0], depth + 1)?,
                                       exact_call(&parts[1], depth + 1)?));
                    }
                    Ok(ModernAst::Conditional { binding, branches })
                }
                CompilerLoweringRole::LambdaForm | CompilerLoweringRole::DefineForm =>
                    Err(ModernLowerError::UnsupportedForm), // binder/locals proof not yet migrated
                _ => {
                    let arity = match binding.role {
                        CompilerLoweringRole::AtomPredicate
                        | CompilerLoweringRole::SelectorHead
                        | CompilerLoweringRole::SelectorTail => 1,
                        CompilerLoweringRole::AtomEquality
                        | CompilerLoweringRole::PairConstruct => 2,
                        _ => return Err(ModernLowerError::UnsupportedForm),
                    };
                    if args.len() != arity { return Err(ModernLowerError::InvalidArity); }
                    Ok(ModernAst::Call {
                        binding,
                        args: args.iter().map(|a| exact_call(a, depth + 1))
                            .collect::<Result<Vec<_>, _>>()?,
                    })
                }
            }
        }
        // For a raw D2 List with a non-callable head, reject instead of
        // inventing a callable identity ("(((0)))" is not an executable call).
        ExprKind::List(_) => Err(ModernLowerError::NonCallableHead),
        _ => Err(ModernLowerError::UnsupportedLiteral),
    }
}

fn lower(ast: &ModernAst) -> Result<ModernIr, ModernLowerError> {
    match ast {
        ModernAst::Literal(data) => Ok(ModernIr::Literal(data.clone())),
        ModernAst::Quote {binding, data} => Ok(ModernIr::Quote {
            binding: binding.clone(), data: data.clone()
        }),
        ModernAst::Call {binding, args} => {
            let target = match binding.role {
                CompilerLoweringRole::AtomPredicate => CmlMechanism::AtomPredicate,
                CompilerLoweringRole::SelectorHead => CmlMechanism::Head,
                CompilerLoweringRole::SelectorTail => CmlMechanism::Tail,
                CompilerLoweringRole::AtomEquality => CmlMechanism::AtomEquality,
                CompilerLoweringRole::PairConstruct => CmlMechanism::PairConstruct,
                _ => return Err(ModernLowerError::UnsupportedForm),
            };
            Ok(ModernIr::Mechanism {
                binding: binding.clone(), target,
                args: args.iter().map(lower).collect::<Result<Vec<_>, _>>()?,
            })
        }
        ModernAst::Conditional {binding, branches} => Ok(ModernIr::Conditional {
            binding: binding.clone(),
            branches: branches.iter().map(|(x, y)| Ok((lower(x)?, lower(y)?)))
                .collect::<Result<Vec<_>, ModernLowerError>>()?,
        }),
    }
}

/// Use only already-verified T5 bytes (from either explicit physical route).
/// SENS owns the canonical reader AND the D3/D4 head normalization.
pub fn lower_verified_program(program: &VerifiedPhysicalProgram)
    -> Result<VerifiedProgramIr, ModernLowerError>
{
    let read = sens::parse_canonical_binary(&program.exact_source)
        .map_err(|_| ModernLowerError::InvalidCanonicalSource)?;
    if read.is_empty() { return Err(ModernLowerError::NoExecutableRoot); }
    let lowered = sens::lower_program(&read);
    let ast = lowered.iter().map(|node| exact_call(node, 0))
        .collect::<Result<Vec<_>, _>>()?;
    let ir = ast.iter().map(lower).collect::<Result<Vec<_>, _>>()?;
    Ok(VerifiedProgramIr {
        ast, ir,
        canonical_t5_sha256: sens::sha256_source(&program.canonical_t5),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VerifiedPhysicalProgram;

    #[test]
    fn generated_registry_is_nine_roles_proved_by_current_sens_not_source_bits() {
        let registry = compiler_registry().unwrap();
        assert_eq!(registry.len(), 9);
        assert_eq!(registry.iter().filter(|r| r.identity.width() == 3).count(), 7);
        assert_eq!(registry.iter().filter(|r| r.identity.width() == 4).count(), 2);
        for row in registry {
            assert!(!row.proof_ref.is_empty());
            assert_eq!(row.authority_sha256.len(), 64);
            assert_eq!(row.authority_path, "language-contract.lisp");
            assert_eq!(
                crate::sens_admitted_compiler_role(row.identity).unwrap(),
                row.role
            );
        }
        println!("CML_MODERN_REGISTRY_PASS source=upstream_D3_D4_9 roles=law_verified");
    }

    #[test]
    fn tiny_f3_and_t5_lower_to_identical_proof_carrying_cml_ir() {
        let t5 = [0x63, 0x89, 0x06, 0xa1];
        let f3 = [0xf3, 0x40, 0x60];
        let ordinary = VerifiedPhysicalProgram::from_t5(&t5).unwrap();
        let compact = VerifiedPhysicalProgram::from_explicit_tiny_f3(
            &f3, sens::sha256_source(&f3)
        ).unwrap();
        let a = lower_verified_program(&ordinary).unwrap();
        let b = lower_verified_program(&compact).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.ir.len(), 1);
        match &a.ir[0] {
            ModernIr::Quote {binding, data: ExactQuotedData::EmptyList} =>
                assert_eq!(binding.role, CompilerLoweringRole::QuoteForm),
            other => panic!("quote must preserve data and proof: {other:?}"),
        }
        println!("CML_MODERN_IR_PARITY_PASS t5=4 f3=3 exact_quote=true");
    }

    #[test]
    fn nonsensical_nested_head_rejected_but_same_structure_quoted_is_preserved() {
        // No executable nested-list head; "( ((0)) )" is structural data,
        // not an admitted function. The canonical D2 reader CAN represent it.
        let residual = "10 10 10 0 01 01 01";
        let t5 = sens::encode_binary_projection_ternary(residual).unwrap();
        let p = VerifiedPhysicalProgram::from_t5(&t5).unwrap();
        assert_eq!(lower_verified_program(&p), Err(ModernLowerError::NonCallableHead));
        // Quoting the SAME structure grants data semantics, not callability.
        let quoted = "10 001 10 10 10 0 01 01 01 01";
        let t5_quote = sens::encode_binary_projection_ternary(quoted).unwrap();
        let q = VerifiedPhysicalProgram::from_t5(&t5_quote).unwrap();
        match &lower_verified_program(&q).unwrap().ir[0] {
            ModernIr::Quote {data: ExactQuotedData::ProperList(_), ..} => (),
            other => panic!("quoted residual must stay quoted data: {other:?}"),
        }
    }

    #[test]
    fn invalid_arity_and_non_core_words_cannot_be_compiled_by_host_guesswork() {
        let wrong_atom_arity = "10 010 01";
        let w = sens::encode_binary_projection_ternary(wrong_atom_arity).unwrap();
        assert_eq!(lower_verified_program(&VerifiedPhysicalProgram::from_t5(&w).unwrap()),
            Err(ModernLowerError::InvalidArity));
        let other_domain = "10 00000010 0 01";
        let x = sens::encode_binary_projection_ternary(other_domain).unwrap();
        assert!(lower_verified_program(&VerifiedPhysicalProgram::from_t5(&x).unwrap()).is_err());
    }
}
