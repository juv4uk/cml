use cml::compiler_mechanism::{RichCompilerMechanismRef, select_rich_compiler_mechanism};
use cml::sens_domain_bridge::{
    CompilerLoweringRequest, MechanismStatus, SemanticStatus, authoritative_lowering_role,
    pinned_authority, verify_lowering_request,
};

fn d3(raw: u8) -> sens::DomainIdentity {
    sens::DomainIdentity::D3(sens::Bija3::from_word(
        sens::Bit3::new(raw).expect("valid D3 identity"),
    ))
}

fn d4(raw: u8) -> sens::DomainIdentity {
    sens::DomainIdentity::D4(sens::CoreD4::from_word(
        sens::Bit4::new(raw).expect("valid D4 identity"),
    ))
}

fn request(identity: sens::DomainIdentity) -> CompilerLoweringRequest {
    let role = authoritative_lowering_role(identity).expect("SENS must admit current role");
    let proof_ref = match role {
        sens::CompilerLoweringRole::LambdaForm | sens::CompilerLoweringRole::DefineForm => {
            "contracts/d4-bootstrap-ratification.lisp"
        }
        _ => "contracts/bija3-l1-l5-ratification.lisp",
    };

    CompilerLoweringRequest {
        identity,
        lowering_role: role,
        law_ref: "lib/compiler-nucleus.lisp:compiler-lowering-role-from-laws".into(),
        proof_ref: proof_ref.into(),
        semantic_status: SemanticStatus::Current,
        mechanism_status: MechanismStatus::Admitted,
        provenance: pinned_authority().expect("pinned SENS provenance"),
    }
}

#[test]
fn current_nucleus_role_closure_reaches_one_cml_rich_mechanism_per_role() {
    let cases = [
        (d3(0b001), RichCompilerMechanismRef::Quote),
        (d3(0b010), RichCompilerMechanismRef::AtomPredicateD1),
        (d3(0b011), RichCompilerMechanismRef::SelectorTail),
        (d3(0b100), RichCompilerMechanismRef::SelectorHead),
        (d3(0b101), RichCompilerMechanismRef::AtomEqualityD1),
        (d3(0b110), RichCompilerMechanismRef::CondExactD1),
        (d3(0b111), RichCompilerMechanismRef::PairConstruct),
        (d4(0b0010), RichCompilerMechanismRef::Lambda),
        (d4(0b0011), RichCompilerMechanismRef::Define),
    ];

    for (identity, expected_mechanism) in cases {
        let verified = verify_lowering_request(request(identity))
            .expect("verified SENS lowering must bind to a rich mechanism");
        assert_eq!(verified.mechanism_ref(), expected_mechanism);
        assert_eq!(
            verified.mechanism_ref().as_str(),
            select_rich_compiler_mechanism(verified.lowering_role()).as_str()
        );
    }
}

#[test]
fn empty_and_non_bootstrap_d4_are_not_current_compiler_roles() {
    assert!(
        authoritative_lowering_role(d3(0b000)).is_err(),
        "D3 structural empty must remain data, not a compiler role"
    );
    assert!(
        authoritative_lowering_role(d4(0b0111)).is_err(),
        "D4 CDDR is not admitted to the current compiler nucleus role closure"
    );
}

#[test]
fn same_payload_in_wrong_domain_cannot_inherit_the_rich_role() {
    let mut bad = request(d4(0b0010));
    bad.lowering_role = sens::CompilerLoweringRole::QuoteForm;
    bad.proof_ref = "contracts/bija3-l1-l5-ratification.lisp".into();

    assert_eq!(
        verify_lowering_request(bad).unwrap_err(),
        cml::sens_domain_bridge::BridgeError::ExecutionRoleMismatch
    );
}

#[test]
fn current_nucleus_source_is_not_a_second_cml_role_table() {
    let source = include_str!("../external/sens/lib/compiler-nucleus.lisp");
    assert!(source.contains("compiler-lowering-role-from-laws"));
    for role in [
        "quote-form",
        "atom-predicate",
        "selector-tail",
        "selector-head",
        "atom-equality",
        "cond-form",
        "pair-construct",
        "lambda-form",
        "define-form",
    ] {
        assert!(
            source.contains(role),
            "SENS nucleus must expose the role {role}"
        );
    }

    let bridge = include_str!("../src/sens_domain_bridge.rs");
    assert!(bridge.contains("sens::compiler_lowering_role_from_sens("));
    assert!(!bridge.contains("packed_bits"));
    assert!(!bridge.contains("compiler_role_from_l1_l5"));
}
