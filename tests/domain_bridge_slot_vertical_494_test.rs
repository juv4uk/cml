//! cml#494 / sens#3758 — exact four-case D3 selector vertical.
//!
//! Cases are read directly from the SENS-owned machine-readable corpus through
//! the pinned external/sens gitlink. The CML path compiles canonical source
//! directly to SLOT-VM; the live pinned SENS evaluator is differential oracle
//! only and never constructs compiler operands.

use cml::compiler_artifact::SlotArtifactEnvelope;
use cml::parser;
use cml::sens_domain_bridge::pinned_authority;
use cml::sens_slot_bridge::lower_canonical_expr_to_slot_program;
use cml::slot_vm::{SlotVmError, execute};
use sens::{ErrorKind, Session};

const CORPUS: &str =
    include_str!("../external/sens/contracts/compiler-d3-selector-corpus-v1.tsv");

const TEST_CML_REVISION: &str = "7572dbc0868d69de4c3c10d4542b635c050bed0f";

struct Case {
    name: String,
    source: String,
    digest: String,
}

fn cases() -> Vec<Case> {
    CORPUS
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(
                fields.len(),
                6,
                "pinned SENS compiler corpus row must have six TSV fields: {line:?}"
            );

            // CML intentionally ignores head_bits and expected semantics here.
            // Identity and execution roles come from the pinned SENS APIs, and
            // the live pinned evaluator remains the semantic oracle.
            Case {
                name: fields[0].to_string(),
                source: fields[2].to_string(),
                digest: fields[3].to_string(),
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Observation {
    Value(String),
    Type,
}

fn sha256_hex(bytes: &[u8]) -> String {
    sens::sha256_source(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn sens_observation(source: &str) -> Observation {
    let parsed = sens::parse_canonical_binary(source).expect("pinned SENS canonical source");
    let lowered = sens::lower_program(&parsed);
    let mut session = Session::default();
    sens::load_core_library(&mut session).expect("pinned SENS core bootstrap");

    match sens::eval_lowered_expressions(&lowered, &mut session) {
        Ok(result) => Observation::Value(result.value.to_string()),
        Err(error) if error.kind == ErrorKind::Type => Observation::Type,
        Err(error) => panic!("unexpected SENS oracle failure: {error:?}"),
    }
}

fn slot_observation(source: &str, digest: &str) -> Observation {
    let parsed = parser::parse_canonical_binary(source)
        .expect("CML must consume the pinned SENS canonical reader output");
    let [expr] = parsed.as_slice() else {
        panic!("vertical source must contain one canonical expression");
    };

    let program = lower_canonical_expr_to_slot_program(expr, digest)
        .expect("canonical source must lower directly to verified SLOT-VM");
    assert_eq!(program.source_case_id.as_deref(), Some(digest));

    let authority = pinned_authority().expect("pinned SENS authority must resolve");
    let envelope = SlotArtifactEnvelope::new(program, &authority, TEST_CML_REVISION)
        .expect("verified SLOT program must accept compiler provenance");
    let encoded = envelope
        .encode_v1()
        .expect("provenance-wrapped SLOT artifact must encode");
    let decoded = SlotArtifactEnvelope::decode_verified_v1(&encoded)
        .expect("provenance-wrapped SLOT artifact must verify and decode");

    assert_eq!(decoded.provenance.program_digest, digest);
    assert_eq!(decoded.provenance.sens_revision, authority.revision);
    assert_eq!(
        decoded.provenance.sens_authority_sha256,
        authority.authority_sha256
    );
    assert_eq!(
        decoded.provenance.sens_contract_version,
        authority.language_contract_version
    );
    assert_eq!(decoded.provenance.cml_revision, TEST_CML_REVISION);
    assert_eq!(decoded.program.source_case_id.as_deref(), Some(digest));

    match execute(&decoded.program) {
        Ok(result) => {
            assert_eq!(result.source_case_id.as_deref(), Some(digest));
            Observation::Value(result.value.to_string())
        }
        Err(SlotVmError::Type { .. }) => Observation::Type,
        Err(error) => panic!("unexpected SLOT-VM failure: {error:?}"),
    }
}

#[test]
fn exact_sens_d3_corpus_compiles_source_to_slot_and_matches_oracle() {
    for case in cases() {
        assert_eq!(
            sha256_hex(case.source.as_bytes()),
            case.digest,
            "{} source drifted from SENS-owned provenance",
            case.name
        );

        let oracle = sens_observation(&case.source);
        let compiled = slot_observation(&case.source, &case.digest);

        assert_eq!(
            compiled, oracle,
            "{} evaluator and compiled SLOT-VM observations diverged",
            case.name
        );
    }
}
