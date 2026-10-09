use wsm_os_target::{BoxedKind, BoxedPredicateBit, ErrorCode, PREDICATE_BIT_BITS, Tag};

#[test]
fn pinned_target_abi_is_current_v9_predicate_runtime_authority() {
    assert_eq!(wsm_os_target::CONTRACT_SCHEMA, "wsm-os-target-v1");
    assert_eq!(wsm_os_target::CONTRACT_VERSION, 9);
    assert_eq!(Tag::Boxed as u8, 7);
    assert_eq!(BoxedKind::PredicateBit as u8, 5);
    assert_eq!(PREDICATE_BIT_BITS, 1);
    assert_eq!(ErrorCode::NumericOverflow as u32, 5);

    let bit0 = BoxedPredicateBit::new(0).expect("v9 admits exact bit 0");
    let bit1 = BoxedPredicateBit::new(1).expect("v9 admits exact bit 1");
    assert_eq!(bit0.kind, BoxedKind::PredicateBit);
    assert_eq!(bit1.kind, BoxedKind::PredicateBit);
    assert_eq!(bit0.exact_bit(), 0);
    assert_eq!(bit1.exact_bit(), 1);
    assert_eq!(BoxedPredicateBit::new(2), None);

    for required in [
        "wsm_rational_new",
        "wsm_rational_numerator",
        "wsm_rational_denominator",
        "wsm_predicate_bit_0",
        "wsm_predicate_bit_1",
        "wsm_predicate_bit_bits",
        "wsm_atom_predicate_bit",
        "wsm_eq_predicate_bit",
    ] {
        assert!(
            wsm_os_target::RUNTIME_IMPORTS.contains(&required),
            "pinned target ABI omitted required representation import: {required}"
        );
    }

    assert!(
        wsm_os_target::CONTRACT_PROJECTION.contains("(numeric-overflow . 5)"),
        "pinned machine-readable target projection omitted NumericOverflow=5"
    );
    assert!(
        wsm_os_target::CONTRACT_PROJECTION.contains("predicate"),
        "pinned machine-readable target projection omitted PredicateBit representation"
    );
}
