//! Compile the ACTUAL production CML Core-profile source against modern SENS
//! without building/hiding any of the 142 blocked historical backend sites.
//! The tested source remains src/core_profile.rs -- no shadow implementation.
#[path = "../../../src/core_profile.rs"]
mod production_core_profile;

use production_core_profile::{CoreProfile, ProfileCompileRequest, ProfileRequestError};

fn d3_eq() -> sens::DomainIdentity {
    // D3 101 is SENS's ratified atom EQ; this is NOT historical byte mapping.
    sens::DomainIdentity::D3(
        sens::Bija3::from_word(sens::Bit3::new(0b101).unwrap()),
    )
}

fn compile_request(identity: sens::DomainIdentity) -> ProfileCompileRequest {
    ProfileCompileRequest::new(
        identity,
        CoreProfile::Core1,
        "390f313242b538cb954dc00afa32ef0003f7d94b",
        "contracts/compiler-semantic-input-v1.lisp",
        "sha256:trusted-current-law",
        "x86_64-freestanding",
        "wsm-value-abi/v1",
    ).unwrap()
}

#[test]
fn production_profile_compiles_against_exact_domain_identity() {
    let request = compile_request(d3_eq());
    assert_eq!(request.semantic_id.width(), 3);
    assert_eq!(request.semantic_id.packed_bits(), 0b101);
    assert_eq!(request.core_profile, CoreProfile::Core1);
    println!("CML_PRODUCTION_CORE_PROFILE_MODERN_PASS exact_D3_101=true");
}

#[test]
fn exact_same_numeric_payload_in_two_domains_does_not_collapse() {
    let d3 = sens::DomainIdentity::D3(
        sens::Bija3::from_word(sens::Bit3::new(0b001).unwrap()),
    );
    let d4 = sens::DomainIdentity::D4(
        sens::CoreD4::from_word(sens::Bit4::new(0b0001).unwrap()),
    );
    assert_eq!(d3.packed_bits(), d4.packed_bits());
    assert_ne!(d3.width(), d4.width());
    let a = compile_request(d3);
    let b = compile_request(d4);
    assert_ne!(a.semantic_id, b.semantic_id);
    assert_ne!(a, b);
    println!("CML_PRODUCTION_PROFILE_WIDTH_COLLISION_REJECTED d3_001_d4_0001");
}

#[test]
fn current_production_profile_rejects_missing_provenance_or_moving_ref() {
    let sha = "390f313242b538cb954dc00afa32ef0003f7d94b";
    for (revision, law, digest, target, abi) in [
        ("main", "law", "digest", "cpu", "abi"),
        ("", "law", "digest", "cpu", "abi"),
        (sha, "", "digest", "cpu", "abi"),
        (sha, "law", "", "cpu", "abi"),
        (sha, "law", "digest", "", "abi"),
        (sha, "law", "digest", "cpu", ""),
    ] {
        assert!(ProfileCompileRequest::new(
            d3_eq(), CoreProfile::Core4, revision, law, digest, target, abi
        ).is_err());
    }
    assert_eq!(
        ProfileCompileRequest::new(
            d3_eq(), CoreProfile::Core4, "main", "law", "digest", "cpu", "abi"
        ).unwrap_err(),
        ProfileRequestError::InvalidUpstreamCommit
    );
}
