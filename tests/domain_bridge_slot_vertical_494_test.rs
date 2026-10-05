//! cml#494 / sens#3758 — exact four-case D3 selector vertical.
//!
//! Cases are read directly from the SENS-owned machine-readable corpus through
//! the pinned external/sens gitlink. Expected semantics are NOT consumed here:
//! the live pinned SENS evaluator computes the oracle observation for each source.

use cml::ast::Expr as CExpr;
use cml::ir::{Ir, Quoted};
use cml::parser;
use cml::sens_domain_bridge::{
    MechanismStatus, SemanticRequest, SemanticStatus, pinned_authority, verify_call,
};
use cml::sens_slot_bridge::lower_verified_call_to_slot_program;
use cml::slot_vm::{SlotVmError, execute};
use sens::{ErrorKind, ExprKind, Session, Value};

const CORPUS: &str =
    include_str!("../external/sens/contracts/compiler-d3-selector-corpus-v1.tsv");

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
            // Identity is parsed from canonical source, role comes from SENS,
            // and the live pinned evaluator remains the semantic oracle.
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

fn cml_head_identity(source: &str) -> sens::DomainIdentity {
    let parsed = parser::parse_canonical_binary(source)
        .expect("CML must consume the pinned SENS canonical reader output");
    let [CExpr::List(items)] = parsed.as_slice() else {
        panic!("vertical source must be one canonical call");
    };
    let Some(CExpr::DomainIdentity(identity)) = items.first() else {
        panic!("canonical call head must preserve exact DomainIdentity");
    };
    *identity
}

fn sens_argument_value(source: &str) -> Value {
    let parsed = sens::parse_canonical_binary(source).expect("pinned SENS canonical source");
    let [expr] = parsed.as_slice() else {
        panic!("vertical source must contain one expression");
    };
    let ExprKind::List(items) = &expr.kind else {
        panic!("vertical source must be one list call");
    };
    let Some(argument) = items.get(1).cloned() else {
        panic!("selector corpus case must have one argument");
    };

    let lowered = sens::lower_program(std::slice::from_ref(&argument));
    let mut session = Session::default();
    sens::load_core_library(&mut session).expect("pinned SENS core bootstrap");
    sens::eval_lowered_expressions(&lowered, &mut session)
        .expect("selector operand construction must evaluate before target selection")
        .value
}

fn quote_runtime_value(value: &Value) -> Quoted {
    match value {
        Value::Nil => Quoted::Nil,
        Value::Pair(_, _) => quote_pair(value),
        other => panic!("first selector corpus operand must be pair/nil, got {other:?}"),
    }
}

fn quote_pair(value: &Value) -> Quoted {
    let mut items = Vec::new();
    let mut cursor = value;

    loop {
        match cursor {
            Value::Pair(head, tail) => {
                items.push(quote_runtime_value(head));
                cursor = tail.as_ref();
            }
            Value::Nil => return Quoted::List(items),
            other => {
                return Quoted::DottedList(items, Box::new(quote_runtime_value(other)));
            }
        }
    }
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
    let identity = cml_head_identity(source);
    let core = identity
        .core_operation()
        .expect("first selector corpus head must be callable Core identity");
    let role = sens::compiler_execution_role(core)
        .expect("first selector corpus head must have SENS compiler role");

    let argument = quote_runtime_value(&sens_argument_value(source));
    let call = verify_call(
        SemanticRequest {
            identity,
            execution_role: role,
            law_ref: "language-contract.lisp:d3-foundation".into(),
            proof_ref: "contracts/bija3-l1-l5-ratification.lisp".into(),
            semantic_status: SemanticStatus::Current,
            mechanism_status: MechanismStatus::Admitted,
            provenance: pinned_authority().expect("CML must describe its exact SENS pin"),
        },
        vec![Ir::Quote(argument)],
    )
    .expect("canonical selector request must verify before target lowering");

    let program = lower_verified_call_to_slot_program(&call, digest)
        .expect("verified selector call must lower mechanically to SLOT-VM");
    assert_eq!(program.source_case_id.as_deref(), Some(digest));

    let encoded = program.encode_v1().expect("verified SLOT artifact must encode");
    let decoded = cml::slot_vm::SlotProgram::decode_v1(&encoded)
        .expect("encoded vertical artifact must decode exactly");
    assert_eq!(decoded.source_case_id.as_deref(), Some(digest));

    match execute(&decoded) {
        Ok(result) => {
            assert_eq!(result.source_case_id.as_deref(), Some(digest));
            Observation::Value(result.value.to_string())
        }
        Err(SlotVmError::Type { .. }) => Observation::Type,
        Err(error) => panic!("unexpected SLOT-VM failure: {error:?}"),
    }
}

#[test]
fn exact_sens_d3_corpus_matches_slot_vm_after_verified_role_bridge() {
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
            "{} evaluator and verified SLOT-VM observations diverged",
            case.name
        );
    }
}
