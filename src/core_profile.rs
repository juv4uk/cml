//! Core-profile compiler request provenance.
//!
//! CML owns compilation mechanism, not language-law selection. A semantic SID
//! therefore cannot be sufficient input for profile-sensitive compilation.
//! This module carries an explicit Core profile and exact upstream law
//! provenance without defining any SID -> meaning or Core -> meaning table.

use sens::Sens8;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoreProfile {
    Core1,
    Core2,
    Core3,
    Core4,
}

impl CoreProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Core1 => "core1",
            Self::Core2 => "core2",
            Self::Core3 => "core3",
            Self::Core4 => "core4",
        }
    }
}

impl std::fmt::Display for CoreProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CoreProfile {
    type Err = ProfileRequestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "core1" => Ok(Self::Core1),
            "core2" => Ok(Self::Core2),
            "core3" => Ok(Self::Core3),
            "core4" => Ok(Self::Core4),
            other => Err(ProfileRequestError::UnsupportedCoreProfile(
                other.to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileCompileRequest {
    pub semantic_id: Sens8,
    pub core_profile: CoreProfile,
    pub upstream_commit: String,
    pub upstream_law_ref: String,
    pub upstream_law_digest: String,
    pub target_backend: String,
    pub target_abi_profile: String,
}

impl ProfileCompileRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        semantic_id: Sens8,
        core_profile: CoreProfile,
        upstream_commit: impl Into<String>,
        upstream_law_ref: impl Into<String>,
        upstream_law_digest: impl Into<String>,
        target_backend: impl Into<String>,
        target_abi_profile: impl Into<String>,
    ) -> Result<Self, ProfileRequestError> {
        let upstream_commit = upstream_commit.into();
        let upstream_law_ref = upstream_law_ref.into();
        let upstream_law_digest = upstream_law_digest.into();
        let target_backend = target_backend.into();
        let target_abi_profile = target_abi_profile.into();

        if !is_exact_git_sha(&upstream_commit) {
            return Err(ProfileRequestError::InvalidUpstreamCommit);
        }
        if upstream_law_ref.trim().is_empty() {
            return Err(ProfileRequestError::MissingUpstreamLawRef);
        }
        if upstream_law_digest.trim().is_empty() {
            return Err(ProfileRequestError::MissingUpstreamLawDigest);
        }
        if target_backend.trim().is_empty() {
            return Err(ProfileRequestError::MissingTargetBackend);
        }
        if target_abi_profile.trim().is_empty() {
            return Err(ProfileRequestError::MissingTargetAbiProfile);
        }

        Ok(Self {
            semantic_id,
            core_profile,
            upstream_commit,
            upstream_law_ref,
            upstream_law_digest,
            target_backend,
            target_abi_profile,
        })
    }
}

fn is_exact_git_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileRequestError {
    UnsupportedCoreProfile(String),
    InvalidUpstreamCommit,
    MissingUpstreamLawRef,
    MissingUpstreamLawDigest,
    MissingTargetBackend,
    MissingTargetAbiProfile,
}

impl std::fmt::Display for ProfileRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedCoreProfile(profile) => {
                write!(f, "unsupported Core profile: {profile}")
            }
            Self::InvalidUpstreamCommit => {
                f.write_str("upstream commit must be an exact 40-hex Git SHA")
            }
            Self::MissingUpstreamLawRef => f.write_str("upstream law reference is required"),
            Self::MissingUpstreamLawDigest => f.write_str("upstream law digest is required"),
            Self::MissingTargetBackend => f.write_str("target/backend is required"),
            Self::MissingTargetAbiProfile => {
                f.write_str("target ABI/profile provenance is required")
            }
        }
    }
}

impl std::error::Error for ProfileRequestError {}
