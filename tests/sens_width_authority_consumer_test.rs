use sens::{DomainIdentity, parse_binary_source_words, semantic_source_bits};

const WIDTH_CERTIFICATE: &str =
    include_str!("../external/sens/knowledge/domain-width-authority.generated.json");

#[test]
fn cml_consumes_upstream_width_authority_and_program_measure() {
    assert!(
        WIDTH_CERTIFICATE.contains("\"generated-projection-non-authoritative\""),
        "the host-visible certificate must remain a projection, not semantic authority"
    );

    let tokens = parse_binary_source_words("10 001 01").expect("canonical SENS source");
    assert_eq!(semantic_source_bits(&tokens), 7);
    assert!(matches!(
        tokens[1].word.domain_identity(),
        DomainIdentity::D3(_)
    ));

    let mixed = parse_binary_source_words("1 10 101 1010 10101 101010 1010101 10101010 100000001")
        .expect("mixed current domains");
    assert_eq!(semantic_source_bits(&mixed), 45);
}
