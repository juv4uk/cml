//! Provenance envelope for emitted compiler artifacts.
//!
//! CMLSLOT1 remains a mechanism-only target artifact. This outer envelope
//! binds those bytes to the exact source/program digest, pinned SENS authority,
//! CML producer revision, and backend profile without putting language meaning
//! into the SLOT-VM format.

use crate::sens_domain_bridge::{AuthorityProvenance, pinned_authority};
use crate::slot_vm::{SlotProgram, SlotVmError};
use std::fmt;

const MAGIC: &[u8; 8] = b"CMLPROV1";
pub const SLOT_BACKEND_ID: &str = "cml.slot-vm";
pub const SLOT_ARTIFACT_FORMAT: &str = "CMLSLOT1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerArtifactProvenance {
    pub program_digest: String,
    pub sens_revision: String,
    pub sens_authority_sha256: String,
    pub sens_contract_version: String,
    pub cml_revision: String,
    pub backend_id: String,
    pub backend_artifact_format: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotArtifactEnvelope {
    pub provenance: CompilerArtifactProvenance,
    pub program: SlotProgram,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactEnvelopeError {
    MissingProgramDigest,
    InvalidProgramDigest,
    InvalidSensRevision,
    InvalidSensAuthorityDigest,
    InvalidCmlRevision,
    UnexpectedBackend,
    UnexpectedArtifactFormat,
    ProgramDigestMismatch,
    SensRevisionMismatch,
    SensAuthorityDigestMismatch,
    SensContractVersionMismatch,
    InvalidMagic,
    InvalidUtf8,
    Truncated,
    LengthOverflow,
    ChecksumMismatch,
    InnerSlotArtifact(SlotVmError),
}

impl fmt::Display for ArtifactEnvelopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ArtifactEnvelopeError {}

impl From<SlotVmError> for ArtifactEnvelopeError {
    fn from(error: SlotVmError) -> Self {
        Self::InnerSlotArtifact(error)
    }
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn validate_provenance(
    provenance: &CompilerArtifactProvenance,
) -> Result<(), ArtifactEnvelopeError> {
    if !is_lower_hex(&provenance.program_digest, 64) {
        return Err(ArtifactEnvelopeError::InvalidProgramDigest);
    }
    if !is_lower_hex(&provenance.sens_revision, 40) {
        return Err(ArtifactEnvelopeError::InvalidSensRevision);
    }
    if !is_lower_hex(&provenance.sens_authority_sha256, 64) {
        return Err(ArtifactEnvelopeError::InvalidSensAuthorityDigest);
    }
    if !is_lower_hex(&provenance.cml_revision, 40) {
        return Err(ArtifactEnvelopeError::InvalidCmlRevision);
    }
    if provenance.backend_id != SLOT_BACKEND_ID {
        return Err(ArtifactEnvelopeError::UnexpectedBackend);
    }
    if provenance.backend_artifact_format != SLOT_ARTIFACT_FORMAT {
        return Err(ArtifactEnvelopeError::UnexpectedArtifactFormat);
    }
    Ok(())
}

impl SlotArtifactEnvelope {
    pub fn new(
        program: SlotProgram,
        authority: &AuthorityProvenance,
        cml_revision: impl Into<String>,
    ) -> Result<Self, ArtifactEnvelopeError> {
        let program_digest = program
            .source_case_id
            .clone()
            .ok_or(ArtifactEnvelopeError::MissingProgramDigest)?;

        let provenance = CompilerArtifactProvenance {
            program_digest,
            sens_revision: authority.revision.clone(),
            sens_authority_sha256: authority.authority_sha256.clone(),
            sens_contract_version: authority.language_contract_version.clone(),
            cml_revision: cml_revision.into(),
            backend_id: SLOT_BACKEND_ID.to_string(),
            backend_artifact_format: SLOT_ARTIFACT_FORMAT.to_string(),
        };
        validate_provenance(&provenance)?;
        Self::validate_program_digest(&program, &provenance)?;

        Ok(Self {
            provenance,
            program,
        })
    }

    fn validate_program_digest(
        program: &SlotProgram,
        provenance: &CompilerArtifactProvenance,
    ) -> Result<(), ArtifactEnvelopeError> {
        match program.source_case_id.as_deref() {
            Some(inner) if inner == provenance.program_digest => Ok(()),
            _ => Err(ArtifactEnvelopeError::ProgramDigestMismatch),
        }
    }

    pub fn encode_v1(&self) -> Result<Vec<u8>, ArtifactEnvelopeError> {
        validate_provenance(&self.provenance)?;
        Self::validate_program_digest(&self.program, &self.provenance)?;

        let inner = self.program.encode_v1()?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        push_string(&mut bytes, &self.provenance.program_digest)?;
        push_string(&mut bytes, &self.provenance.sens_revision)?;
        push_string(&mut bytes, &self.provenance.sens_authority_sha256)?;
        push_string(&mut bytes, &self.provenance.sens_contract_version)?;
        push_string(&mut bytes, &self.provenance.cml_revision)?;
        push_string(&mut bytes, &self.provenance.backend_id)?;
        push_string(&mut bytes, &self.provenance.backend_artifact_format)?;
        push_bytes(&mut bytes, &inner)?;

        let checksum = sens::sha256_source(&bytes);
        bytes.extend_from_slice(&checksum);
        Ok(bytes)
    }

    /// Decode and verify the envelope against the SENS authority pinned into
    /// this CML build. The CML producer revision is recorded and format-checked;
    /// it is intentionally supplied by the producer rather than guessed here.
    pub fn decode_verified_v1(bytes: &[u8]) -> Result<Self, ArtifactEnvelopeError> {
        if bytes.len() < MAGIC.len() + 32 {
            return Err(ArtifactEnvelopeError::Truncated);
        }
        let (body, checksum) = bytes.split_at(bytes.len() - 32);
        if sens::sha256_source(body).as_slice() != checksum {
            return Err(ArtifactEnvelopeError::ChecksumMismatch);
        }

        let mut decoder = Decoder::new(body);
        if decoder.take(MAGIC.len())? != MAGIC {
            return Err(ArtifactEnvelopeError::InvalidMagic);
        }

        let provenance = CompilerArtifactProvenance {
            program_digest: decoder.take_string()?,
            sens_revision: decoder.take_string()?,
            sens_authority_sha256: decoder.take_string()?,
            sens_contract_version: decoder.take_string()?,
            cml_revision: decoder.take_string()?,
            backend_id: decoder.take_string()?,
            backend_artifact_format: decoder.take_string()?,
        };
        validate_provenance(&provenance)?;

        let inner = decoder.take_blob()?;
        if !decoder.is_empty() {
            return Err(ArtifactEnvelopeError::LengthOverflow);
        }
        let program = SlotProgram::decode_v1(inner)?;
        Self::validate_program_digest(&program, &provenance)?;

        let authority =
            pinned_authority().map_err(|_| ArtifactEnvelopeError::SensRevisionMismatch)?;
        if provenance.sens_revision != authority.revision {
            return Err(ArtifactEnvelopeError::SensRevisionMismatch);
        }
        if provenance.sens_authority_sha256 != authority.authority_sha256 {
            return Err(ArtifactEnvelopeError::SensAuthorityDigestMismatch);
        }
        if provenance.sens_contract_version != authority.language_contract_version {
            return Err(ArtifactEnvelopeError::SensContractVersionMismatch);
        }

        Ok(Self {
            provenance,
            program,
        })
    }
}

fn push_string(bytes: &mut Vec<u8>, value: &str) -> Result<(), ArtifactEnvelopeError> {
    push_bytes(bytes, value.as_bytes())
}

fn push_bytes(bytes: &mut Vec<u8>, value: &[u8]) -> Result<(), ArtifactEnvelopeError> {
    let len = u32::try_from(value.len()).map_err(|_| ArtifactEnvelopeError::LengthOverflow)?;
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(value);
    Ok(())
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], ArtifactEnvelopeError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(ArtifactEnvelopeError::LengthOverflow)?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(ArtifactEnvelopeError::Truncated)?;
        self.offset = end;
        Ok(slice)
    }

    fn take_u32(&mut self) -> Result<u32, ArtifactEnvelopeError> {
        let raw = self.take(4)?;
        Ok(u32::from_le_bytes(
            raw.try_into().expect("exact 4-byte slice"),
        ))
    }

    fn take_blob(&mut self) -> Result<&'a [u8], ArtifactEnvelopeError> {
        let len =
            usize::try_from(self.take_u32()?).map_err(|_| ArtifactEnvelopeError::LengthOverflow)?;
        self.take(len)
    }

    fn take_string(&mut self) -> Result<String, ArtifactEnvelopeError> {
        let raw = self.take_blob()?;
        std::str::from_utf8(raw)
            .map(str::to_string)
            .map_err(|_| ArtifactEnvelopeError::InvalidUtf8)
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot_vm::{Slot, SlotInstr};

    const CML_REV_A: &str = "1111111111111111111111111111111111111111";
    const CML_REV_B: &str = "2222222222222222222222222222222222222222";
    const PROGRAM_DIGEST: &str = "0b36aad2d404292ab70ce7510f103d51dc5ec02ac9e8e7281dcddbe818c8deb3";

    fn program() -> SlotProgram {
        SlotProgram {
            source_case_id: Some(PROGRAM_DIGEST.to_string()),
            slot_count: 1,
            instructions: vec![
                SlotInstr::LoadNil { dst: Slot::new(0) },
                SlotInstr::Return { src: Slot::new(0) },
            ],
        }
    }

    #[test]
    fn envelope_round_trips_against_pinned_sens_authority() {
        let authority = pinned_authority().unwrap();
        let envelope = SlotArtifactEnvelope::new(program(), &authority, CML_REV_A).unwrap();
        let encoded = envelope.encode_v1().unwrap();
        let decoded = SlotArtifactEnvelope::decode_verified_v1(&encoded).unwrap();

        assert_eq!(decoded, envelope);
        assert_eq!(decoded.provenance.backend_id, SLOT_BACKEND_ID);
        assert_eq!(
            decoded.provenance.backend_artifact_format,
            SLOT_ARTIFACT_FORMAT
        );
    }

    #[test]
    fn envelope_detects_byte_tampering() {
        let authority = pinned_authority().unwrap();
        let envelope = SlotArtifactEnvelope::new(program(), &authority, CML_REV_A).unwrap();
        let mut encoded = envelope.encode_v1().unwrap();
        let index = MAGIC.len() + 8;
        encoded[index] ^= 1;

        assert_eq!(
            SlotArtifactEnvelope::decode_verified_v1(&encoded).unwrap_err(),
            ArtifactEnvelopeError::ChecksumMismatch
        );
    }

    #[test]
    fn recomputed_checksum_cannot_hide_a_stale_sens_revision() {
        let authority = pinned_authority().unwrap();
        let mut envelope = SlotArtifactEnvelope::new(program(), &authority, CML_REV_A).unwrap();
        envelope.provenance.sens_revision = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();

        // Encoding is structurally valid and gets a fresh checksum, but decode
        // still verifies semantic provenance against the SENS authority pinned
        // into this CML build.
        let encoded = envelope.encode_v1().unwrap();
        assert_eq!(
            SlotArtifactEnvelope::decode_verified_v1(&encoded).unwrap_err(),
            ArtifactEnvelopeError::SensRevisionMismatch
        );
    }

    #[test]
    fn program_digest_must_match_inner_slot_artifact() {
        let authority = pinned_authority().unwrap();
        let mut envelope = SlotArtifactEnvelope::new(program(), &authority, CML_REV_A).unwrap();
        envelope.provenance.program_digest =
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();

        assert_eq!(
            envelope.encode_v1().unwrap_err(),
            ArtifactEnvelopeError::ProgramDigestMismatch
        );
    }

    #[test]
    fn same_slot_bytes_with_different_cml_provenance_are_distinguishable() {
        let authority = pinned_authority().unwrap();
        let a = SlotArtifactEnvelope::new(program(), &authority, CML_REV_A).unwrap();
        let b = SlotArtifactEnvelope::new(program(), &authority, CML_REV_B).unwrap();

        assert_eq!(
            a.program.encode_v1().unwrap(),
            b.program.encode_v1().unwrap()
        );
        assert_ne!(a.encode_v1().unwrap(), b.encode_v1().unwrap());
    }
}
