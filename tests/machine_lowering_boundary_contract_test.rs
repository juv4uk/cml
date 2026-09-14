//! Machine Lowering Authority Boundary Integration Test (Issue #38 P1)
//!
//! Enforces:
//! - Exact upstream authority: `my-lisp` owns semantic IDs; `cml` owns machine lowering.
//! - Direction is strictly `semantic -> machine`; reverse authority is forbidden.
//! - Compiler mechanisms (e.g. RDTSC, MOV, LEA) cannot allocate or masquerade as semantic IDs.
//! - Retired semantic ID `1153` is rejected fail-closed.

use cml::machine_boundary::{BOUNDARY_SCHEMA, BoundaryError, MachineLoweringBoundary};

#[test]
fn test_vendored_boundary_contract_is_valid() {
    let boundary = MachineLoweringBoundary::load_vendored()
        .expect("vendored machine-lowering-boundary.lisp must parse and validate successfully");

    assert_eq!(boundary.schema, BOUNDARY_SCHEMA);
    assert_eq!(boundary.semantic_authority, "my-lisp");
    assert_eq!(boundary.compiler_authority, "cml");
    assert_eq!(boundary.lowering_direction, "semantic-to-machine");
    assert_eq!(boundary.reverse_authority, "forbidden");
    assert_eq!(boundary.portable_monotonic_observation, "1075");
    assert!(
        boundary.retired_semantic_ids.contains(&"1153".to_string()),
        "1153 must be explicitly tracked as a retired semantic ID"
    );
}

#[test]
fn test_retired_semantic_id_fails_closed() {
    let boundary = MachineLoweringBoundary::load_vendored().unwrap();

    // Validating active ID 1153 must fail closed
    let result = boundary.validate_semantic_id("1153");
    assert_eq!(
        result,
        Err(BoundaryError::RetiredSemanticIdAttempt("1153".to_string())),
        "any attempt to admit or allocate retired ID 1153 must produce a named error"
    );

    // Ordinary permitted ID succeeds
    assert_eq!(boundary.validate_semantic_id("0002"), Ok(()));
    assert_eq!(boundary.validate_semantic_id("1074"), Ok(()));
}

#[test]
fn test_reverse_authority_and_mismatch_detection() {
    // 1. Reverse lowering direction must be rejected
    let bad_direction = r#"
        (machine-lowering-boundary
          (schema machine-lowering-boundary/1)
          (semantic-authority my-lisp)
          (compiler-authority cml)
          (lowering-direction machine-to-semantic)
          (reverse-authority permitted))
    "#;
    assert!(matches!(
        MachineLoweringBoundary::parse(bad_direction),
        Err(BoundaryError::ReverseAuthorityViolation(_))
    ));

    // 2. Usurping semantic authority must be rejected
    let bad_authority = r#"
        (machine-lowering-boundary
          (schema machine-lowering-boundary/1)
          (semantic-authority cml)
          (compiler-authority cml)
          (lowering-direction semantic-to-machine)
          (reverse-authority forbidden))
    "#;
    assert!(matches!(
        MachineLoweringBoundary::parse(bad_authority),
        Err(BoundaryError::AuthorityMismatch {
            expected: "my-lisp",
            ..
        })
    ));
}

#[test]
fn test_compiler_machine_ops_have_no_semantic_id_leakage() {
    use cml::ir::MachineOp;
    use cml::machine_inst::select_machine_primitive;

    // RDTSC is a compiler-owned mechanism, not language semantic authority
    let insts = select_machine_primitive(MachineOp::Rdtsc, 3);
    for inst in insts {
        assert_eq!(
            inst.provenance().semantic_id,
            None,
            "compiler machine primitive must not claim or allocate a language semantic ID"
        );
    }
}
