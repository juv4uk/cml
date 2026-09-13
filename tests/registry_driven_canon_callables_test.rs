use cml::ir::Ir;
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).expect("source must parse");
    let mut lowered = lower::lower_program(&expressions).expect("source must lower");
    assert_eq!(lowered.len(), 1, "fixture must contain one expression");
    lowered.remove(0)
}

#[test]
fn every_admitted_canon_callable_surface_lowers_to_one_semantic_operation() {
    // RED witness for the registry/function-table boundary.
    // These peer spellings are the stable surfaces of semantic IDs 0002..0006
    // in my-lisp/lib/surface/semantic-registry.wsm. The implementation must
    // derive admission from that registry; this test only states the observable
    // contract: changing human spelling must not change the lowered operation.
    let cases: &[(&str, &str, &str, &[&str])] = &[
        ("0002", "atom", "1", &["атом?", "aṇu", ".?"]),
        ("0003", "eq", "1 1", &["тотожне?", "abheda", "=?"]),
        (
            "0004",
            "cons",
            "1 (quote ())",
            &["сполучити", "saṃyuj", ":"],
        ),
        ("0005", "car", "(quote (1 2))", &["перше", "ādi", ":п"]),
        ("0006", "cdr", "(quote (1 2))", &["решта", "śeṣa", ":р"]),
    ];

    for (semantic_id, english, args, peers) in cases {
        let baseline = lower_one(&format!("({english} {args})"));
        for peer in *peers {
            let actual = lower_one(&format!("({peer} {args})"));
            assert_eq!(
                actual, baseline,
                "surface {peer:?} must lower through semantic identity {semantic_id}, not through spelling-specific dispatch"
            );
        }
    }
}
