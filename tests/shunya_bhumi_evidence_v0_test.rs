use std::fs;
use std::path::PathBuf;

fn record() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("evidence/shunya-bhumi/cml-evidence-v0.jsonl");
    fs::read_to_string(path).expect("shared evidence record must be readable")
}

#[test]
fn cml_evidence_record_consumes_shared_v0_without_claiming_semantic_authority() {
    let record = record();

    for required in [
        "\"claim_id\"",
        "\"timestamp\"",
        "\"agent_or_source\"",
        "\"ksetra\"",
        "\"status\"",
        "\"evidence_domain\"",
        "\"evidence_level\"",
        "\"statement\"",
        "\"source_refs\"",
        "\"raw_artifact_refs\"",
        "\"alternative_explanations\"",
    ] {
        assert!(
            record.contains(required),
            "ŚŪNYA-BHŪMI v0 required field missing: {required}"
        );
    }

    assert!(record.contains("\"status\":\"OBSERVED\""));
    assert!(record.contains("\"evidence_level\":\"CI-PASS\""));
    assert!(
        record.contains("juv4uk/ecosystem:schemas/shunya-bhumi-evidence-v0.schema.json"),
        "record must point back to the shared canonical schema"
    );
    assert!(
        record.contains("my-lisp remains semantic authority"),
        "CML kṣetra must explicitly hand semantic authority back to my-lisp"
    );
    assert!(
        record.contains("22cc34e044d715c86475ddc6a27c83a058cd6127")
            && record.contains("35516728928"),
        "software evidence must identify the exact implementation and raw CI run"
    );
}
