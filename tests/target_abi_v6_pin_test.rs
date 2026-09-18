use wsm_os_target::{ErrorCode, Tag};

#[test]
fn pinned_target_abi_is_current_v6_authority() {
    assert_eq!(wsm_os_target::CONTRACT_SCHEMA, "wsm-os-target-v1");
    assert_eq!(wsm_os_target::CONTRACT_VERSION, 6);
    assert_eq!(Tag::Boxed as u8, 7);
    assert_eq!(ErrorCode::NumericOverflow as u32, 5);

    for required in [
        "wsm_rational_new",
        "wsm_rational_numerator",
        "wsm_rational_denominator",
    ] {
        assert!(
            wsm_os_target::RUNTIME_IMPORTS.contains(&required),
            "pinned target ABI omitted required Rational import: {required}"
        );
    }

    assert!(
        wsm_os_target::CONTRACT_PROJECTION.contains("(numeric-overflow . 5)"),
        "pinned machine-readable target projection omitted NumericOverflow=5"
    );
}
