//! Candidate-only CML AST / IR boundary. Read and lower with SENS's canonical
//! D2 reader + SENS-owned exact compiler law; never resurrect byte-opcode SID.
//!
//! This is a strictly bounded compiler subset. Unsupported constructs fail
//! closed: this is not a replacement for the historical CML production backend.
use sens::{CompilerLoweringRole, CoreDomainIdentity, DomainIdentity};
use sens::syntax::{Expr, ExprKind};

use crate::VerifiedPhysicalProgram;

// All current callable semantics, quoted data and target IR come from a
// single SENS-verified root source module, not a copied candidate compiler.
pub use crate::current_exact_model::{
    quoted_data, lower, CmlMechanism, ExactQuotedData, ModernAst, ModernIr,
    ModernLowerError, VerifiedProgramIr, VerifiedRole, exact_call, verified_role,
    lower_canonical_t5_bytes,
};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/sens_compiler_candidates.rs"));
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

/// Use only already-verified T5 bytes (from either explicit physical route).
/// SENS owns the canonical reader AND the D3/D4 head normalization.
pub fn lower_verified_program(program: &VerifiedPhysicalProgram)
    -> Result<VerifiedProgramIr, ModernLowerError>
{
    // The carrier's *physical* T5, not caller-provided exact_source metadata,
    // is the authority. Reject a forged pair where source and bytes differ.
    let actual_source = sens::open_ternary_program(&program.canonical_t5)
        .map_err(|_| ModernLowerError::InvalidCanonicalSource)?;
    if actual_source != program.exact_source {
        return Err(ModernLowerError::InvalidCanonicalSource);
    }
    lower_canonical_t5_bytes(&program.canonical_t5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VerifiedPhysicalProgram;

    #[test]
    fn forged_source_metadata_cannot_rebind_physical_program_meaning() {
        let physical_t5 = [0x63, 0x89, 0x06, 0xa1];
        let verified = VerifiedPhysicalProgram::from_t5(&physical_t5).unwrap();
        let baseline = lower_verified_program(&verified).unwrap();

        let mut swapped_projection = verified.clone();
        swapped_projection.exact_source = "10 000 01".to_string();
        assert_eq!(
            lower_verified_program(&swapped_projection),
            Err(ModernLowerError::InvalidCanonicalSource),
            "physical T5 bytes, not a human-created source string, own AST semantics",
        );

        let mut swapped_physical = verified.clone();
        swapped_physical.canonical_t5 = vec![0xf3, 0x40, 0x60];
        assert_eq!(
            lower_verified_program(&swapped_physical),
            Err(ModernLowerError::InvalidCanonicalSource),
            "F3 bytes cannot be smuggled through canonical T5-only root entry",
        );
        assert_eq!(
            lower_canonical_t5_bytes(&verified.canonical_t5).unwrap(),
            baseline
        );
        println!("CML_ROOT_PHYSICAL_TO_IR_AUTHORITY_PASS forged_source_rejected=true");
    }

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
    fn fabricated_proof_or_role_in_user_constructed_ast_never_reaches_cml_ir() {
        let identity = CoreDomainIdentity::D3(sens::Bija3::from_word(
            sens::Bit3::new(0b001).unwrap()
        ));
        let checked = verified_role(identity).expect("SENS must verify D3 QUOTE");
        assert_eq!(checked.role, CompilerLoweringRole::QuoteForm);

        let proper = ModernAst::Quote {
            binding: checked.clone(),
            data: ExactQuotedData::EmptyList,
        };
        assert!(matches!(lower(&proper), Ok(ModernIr::Quote { .. })));

        // A caller might construct public VerifiedRole fields by hand: never
        // trust the carried role/digest, even when exact domain bits match.
        let mut forged_role = checked.clone();
        forged_role.role = CompilerLoweringRole::PairConstruct;
        assert_eq!(
            lower(&ModernAst::Call { binding: forged_role, args: vec![] }),
            Err(ModernLowerError::InvalidCompilerAuthority)
        );
        let mut forged_digest = checked.clone();
        forged_digest.authority_sha256 = "0".repeat(64);
        assert_eq!(
            lower(&ModernAst::Quote {
                binding: forged_digest,
                data: ExactQuotedData::EmptyList,
            }),
            Err(ModernLowerError::InvalidCompilerAuthority)
        );
        // Even an authentic QUOTE proof must not be used as COND authority.
        assert_eq!(
            lower(&ModernAst::Conditional { binding: checked, branches: vec![] }),
            Err(ModernLowerError::InvalidCompilerAuthority)
        );
        println!("CML_MODERN_FORGED_ROLE_REJECTED all_ast_call_forms=true");
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
