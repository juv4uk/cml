use cml::core_profile::{CoreProfile, ProfileCompileRequest, ProfileRequestError};

#[test]
fn same_sid_can_request_different_core_laws_without_identity_duplication() {
    let sid = sens::sid!(00000011);

    let core1 = ProfileCompileRequest::new(
        sid,
        CoreProfile::Core1,
        "0123456789abcdef0123456789abcdef01234567",
        "contracts/core1/eq-historical.lisp",
        "sha256:core1-eq-law",
        "x86_64-freestanding",
        "wsm-value-abi/v1",
    )
    .expect("complete Core1 provenance must be admitted as a request");

    let core4 = ProfileCompileRequest::new(
        sid,
        CoreProfile::Core4,
        "89abcdef0123456789abcdef0123456789abcdef",
        "contracts/core4/eq-identity-relation.lisp",
        "sha256:core4-eq-law",
        "x86_64-freestanding",
        "wsm-value-abi/v1",
    )
    .expect("complete Core4 provenance must be admitted as a request");

    assert_eq!(core1.semantic_id, core4.semantic_id);
    assert_ne!(core1.core_profile, core4.core_profile);
    assert_ne!(core1.upstream_law_ref, core4.upstream_law_ref);
    assert_ne!(core1, core4);
}

#[test]
fn profile_request_rejects_bare_sid_or_incomplete_upstream_provenance() {
    let sid = sens::sid!(00000011);

    let cases = [
        ("", "contracts/core1/eq.lisp", "sha256:eq", "x86", "abi"),
        (
            "0123456789abcdef0123456789abcdef01234567",
            "",
            "sha256:eq",
            "x86",
            "abi",
        ),
        (
            "0123456789abcdef0123456789abcdef01234567",
            "contracts/core1/eq.lisp",
            "",
            "x86",
            "abi",
        ),
        (
            "0123456789abcdef0123456789abcdef01234567",
            "contracts/core1/eq.lisp",
            "sha256:eq",
            "",
            "abi",
        ),
        (
            "0123456789abcdef0123456789abcdef01234567",
            "contracts/core1/eq.lisp",
            "sha256:eq",
            "x86",
            "",
        ),
    ];

    for (commit, law_ref, law_digest, target, abi) in cases {
        assert!(
            ProfileCompileRequest::new(
                sid,
                CoreProfile::Core1,
                commit,
                law_ref,
                law_digest,
                target,
                abi,
            )
            .is_err(),
            "incomplete provenance must fail closed: {commit:?} {law_ref:?} {law_digest:?} {target:?} {abi:?}"
        );
    }
}

#[test]
fn upstream_commit_must_be_an_exact_sha_not_a_moving_ref() {
    let err = ProfileCompileRequest::new(
        sens::sid!(00000011),
        CoreProfile::Core1,
        "main",
        "contracts/core1/eq.lisp",
        "sha256:eq",
        "x86_64-freestanding",
        "wsm-value-abi/v1",
    )
    .expect_err("moving upstream refs are insufficient provenance");

    assert_eq!(err, ProfileRequestError::InvalidUpstreamCommit);
}

#[test]
fn core_profile_parse_is_explicit_and_fail_closed() {
    assert_eq!("core1".parse::<CoreProfile>().unwrap(), CoreProfile::Core1);
    assert_eq!("core2".parse::<CoreProfile>().unwrap(), CoreProfile::Core2);
    assert_eq!("core3".parse::<CoreProfile>().unwrap(), CoreProfile::Core3);
    assert_eq!("core4".parse::<CoreProfile>().unwrap(), CoreProfile::Core4);

    assert!("current".parse::<CoreProfile>().is_err());
    assert!("historical".parse::<CoreProfile>().is_err());
    assert!("".parse::<CoreProfile>().is_err());
}

#[test]
fn core_profile_roundtrip_is_byte_stable_for_artifact_metadata() {
    for profile in [
        CoreProfile::Core1,
        CoreProfile::Core2,
        CoreProfile::Core3,
        CoreProfile::Core4,
    ] {
        let text = profile.to_string();
        assert_eq!(text.parse::<CoreProfile>().unwrap(), profile);
        assert_eq!(profile.as_str(), text);
    }
}
