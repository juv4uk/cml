use wsm_os_target::{BoxedKind, ErrorCode, Tag};

#[test]
fn pinned_target_abi_is_current_v7_authority() {
    assert_eq!(wsm_os_target::CONTRACT_SCHEMA, "wsm-os-target-v1");
    assert_eq!(wsm_os_target::CONTRACT_VERSION, 7);
    assert_eq!(Tag::Boxed as u8, 7);
    assert_eq!(BoxedKind::Sid8 as u8, 4);
    assert_eq!(wsm_os_target::SID8_BITS, 8);
    assert_eq!(ErrorCode::NumericOverflow as u32, 5);

    for required in [
        "wsm_rational_new",
        "wsm_rational_numerator",
        "wsm_rational_denominator",
        "wsm_sid8_new",
        "wsm_sid8_bits",
    ] {
        assert!(
            wsm_os_target::RUNTIME_IMPORTS.contains(&required),
            "pinned target ABI omitted required runtime import: {required}"
        );
    }

    assert!(
        wsm_os_target::CONTRACT_PROJECTION.contains("(numeric-overflow . 5)"),
        "pinned machine-readable target projection omitted NumericOverflow=5"
    );
    assert!(
        wsm_os_target::CONTRACT_PROJECTION.contains(
            "(sid8 . ((boxed-kind . 4) (bits . 8) (minimum . 0) (maximum . 255) (identity . exact-bare-8-bit)))"
        ),
        "pinned machine-readable target projection omitted exact bare SID8 contract"
    );
}
