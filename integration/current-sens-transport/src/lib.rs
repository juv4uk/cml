//! Candidate-only, exact SENS transport boundary for future CML domain migration.
//!
//! SENS is the single semantic authority. No byte identities, legacy renumbering,
//! symbol/name identity inference, or second Frame3 implementation exists here.
//! T5 remains the default. Tiny F3 is explicitly selected and must be backed
//! by a SHA-256 acquired from an independently trusted source before decoding.
//!
//! This crate is deliberately buildable with CURRENT SENS while the historical
//! CML backends are migrated. A green test here is NOT a green CML production
//! compiler or a claim of full current CML IR parity.

pub mod modern_ir;

use sens::{CoreDomainIdentity, CompilerLoweringRole};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Carrier {
    DefaultT5,
    ExplicitTinyF3WithTrustedSha256,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdmissionError {
    UntrustedCarrierDigest,
    NonCanonicalOrInvalidT5,
    RejectedTinyF3,
    NotASingleTinyProgram,
    CompilerRoleNotAdmitted,
    CompilerLawFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedPhysicalProgram {
    pub carrier: Carrier,
    /// The ONLY machine-language payload passed to downstream compiler code.
    pub canonical_t5: Vec<u8>,
    /// Canonical exact binary projection produced by SENS (not a surface name).
    pub exact_source: String,
}

fn canonical_t5(input: &[u8]) -> Result<String, AdmissionError> {
    let decoded = sens::PhysicalT5Program::decode(input)
        .map_err(|_| AdmissionError::NonCanonicalOrInvalidT5)?;
    if decoded.form_count() == 0 {
        return Err(AdmissionError::NonCanonicalOrInvalidT5);
    }
    let source = sens::open_ternary_program(input)
        .map_err(|_| AdmissionError::NonCanonicalOrInvalidT5)?;
    let reencoded = sens::encode_binary_projection_ternary(&source)
        .map_err(|_| AdmissionError::NonCanonicalOrInvalidT5)?;
    if reencoded != input {
        return Err(AdmissionError::NonCanonicalOrInvalidT5);
    }
    Ok(source)
}

impl VerifiedPhysicalProgram {
    /// The pre-existing default route never guesses the F3 carrier by magic.
    pub fn from_t5(input: &[u8]) -> Result<Self, AdmissionError> {
        let exact_source = canonical_t5(input)?;
        Ok(Self {
            carrier: Carrier::DefaultT5,
            canonical_t5: input.to_vec(),
            exact_source,
        })
    }

    /// The trusted digest covers the *physical F3 bytes* and MUST be sourced
    /// independently of the file; use a signed manifest or pre-pinned digest.
    /// Verification is done on the same in-memory byte slice consumed by
    /// the authoritative decoder, preventing a path re-read race.
    pub fn from_explicit_tiny_f3(
        physical_f3: &[u8],
        trusted_sha256: [u8; 32],
    ) -> Result<Self, AdmissionError> {
        let computed = sens::sha256_source(physical_f3);
        let mismatch = computed.iter().zip(trusted_sha256.iter())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b));
        if mismatch != 0 {
            return Err(AdmissionError::UntrustedCarrierDigest);
        }

        let restored = sens::frame3_small::decode_explicit_small_f3(physical_f3)
            .map_err(|_| AdmissionError::RejectedTinyF3)?;
        let decoded = sens::PhysicalT5Program::decode(&restored)
            .map_err(|_| AdmissionError::NonCanonicalOrInvalidT5)?;
        if decoded.form_count() != 1 {
            return Err(AdmissionError::NotASingleTinyProgram);
        }
        let exact_source = canonical_t5(&restored)?;
        Ok(Self {
            carrier: Carrier::ExplicitTinyF3WithTrustedSha256,
            canonical_t5: restored,
            exact_source,
        })
    }
}

/// Compiler requests are derived by SENS's executable D3/D4 law, not by
/// matching bits, byte width, human names, or any CML-owned role table.
pub fn sens_admitted_compiler_role(
    exact_identity: CoreDomainIdentity,
) -> Result<CompilerLoweringRole, AdmissionError> {
    sens::compiler_semantic_input_from_sens(exact_identity)
        .map_err(|_| AdmissionError::CompilerLawFailed)?
        .map(|request| request.lowering_role)
        .ok_or(AdmissionError::CompilerRoleNotAdmitted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sens::{Bija3, Bit3, Bit4, CoreD4, CoreD8, Bit8};

    const QUOTE_T5: [u8; 4] = [0x63, 0x89, 0x06, 0xa1];
    const QUOTE_F3: [u8; 3] = [0xf3, 0x40, 0x60];

    #[test]
    fn exact_small_f3_and_default_t5_reach_identical_authoritative_program() {
        let trusted = sens::sha256_source(&QUOTE_F3);
        let ordinary = VerifiedPhysicalProgram::from_t5(&QUOTE_T5).unwrap();
        let compact = VerifiedPhysicalProgram::from_explicit_tiny_f3(&QUOTE_F3, trusted).unwrap();
        assert_eq!(ordinary.carrier, Carrier::DefaultT5);
        assert_eq!(compact.carrier, Carrier::ExplicitTinyF3WithTrustedSha256);
        assert_eq!(compact.canonical_t5, QUOTE_T5);
        assert_eq!(compact.exact_source, ordinary.exact_source);
        let encoded = sens::frame3_small::encode_explicit_small_f3(&ordinary.canonical_t5)
            .unwrap().expect("tiny F3 must save one physical byte");
        assert_eq!(encoded, QUOTE_F3);
        assert!(QUOTE_F3.len() < QUOTE_T5.len());

        let standard = sens::PhysicalT5Program::decode(&ordinary.canonical_t5)
            .unwrap().execute(&mut sens::Session::default()).unwrap();
        let compressed = sens::PhysicalT5Program::decode(&compact.canonical_t5)
            .unwrap().execute(&mut sens::Session::default()).unwrap();
        assert_eq!(standard.value, compressed.value);
        assert_eq!(standard.output, compressed.output);
        println!("CML_MODERN_TRANSPORT_PASS exact_program=t5:4B f3:3B same_output=true");
    }

    #[test]
    fn corrupt_or_untrusted_f3_never_enters_compiler() {
        let trusted = sens::sha256_source(&QUOTE_F3);
        let mut tampered = QUOTE_F3;
        tampered[2] ^= 1;
        assert_eq!(
            VerifiedPhysicalProgram::from_explicit_tiny_f3(&tampered, trusted),
            Err(AdmissionError::UntrustedCarrierDigest)
        );
        let bad_digest = [0u8; 32];
        assert_eq!(
            VerifiedPhysicalProgram::from_explicit_tiny_f3(&QUOTE_F3, bad_digest),
            Err(AdmissionError::UntrustedCarrierDigest)
        );
        for invalid in [
            vec![], vec![0xf3], vec![0xf4, 0x40, 0x60],
            vec![0xf3; 16],
        ] {
            assert!(VerifiedPhysicalProgram::from_explicit_tiny_f3(
                &invalid, sens::sha256_source(&invalid)
            ).is_err(), "malformed physical F3 admitted: {invalid:02x?}");
        }
        assert!(VerifiedPhysicalProgram::from_t5(&QUOTE_F3).is_err());
    }

    #[test]
    fn roles_are_derived_by_domain_width_and_ratified_sens_laws_only() {
        let atom = CoreDomainIdentity::D3(Bija3::from_word(Bit3::new(0b010).unwrap()));
        let lambda = CoreDomainIdentity::D4(CoreD4::from_word(Bit4::new(0b0010).unwrap()));
        assert_eq!(
            sens_admitted_compiler_role(atom).unwrap(),
            CompilerLoweringRole::AtomPredicate
        );
        assert_eq!(
            sens_admitted_compiler_role(lambda).unwrap(),
            CompilerLoweringRole::LambdaForm
        );
        let unsupported = CoreDomainIdentity::D8(
            CoreD8::from_word(Bit8::new(0b0000_0010).unwrap()));
        assert_eq!(
            sens_admitted_compiler_role(unsupported),
            Err(AdmissionError::CompilerRoleNotAdmitted)
        );
        println!("CML_MODERN_DOMAIN_AUTHORITY_PASS D3_D4_width_distinct=true D8_rejected=true");
    }
}
