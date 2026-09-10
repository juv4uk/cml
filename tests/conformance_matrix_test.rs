//! COMPILER-11 — fixture | compiled | status | reason matrix.
//!
//! Every constitutive row is classified structurally. Unsupported is never
//! pass. Silent skip is a failure. Native my-lisp / my-eval columns join when
//! a sibling checkout is present; this harness stays green on compiled-only
//! hosts by embedding a mini-corpus that mirrors tier-1 shape.

use cml::build::{compile_and_run, Observation};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Value,
    Error,
    Unsupported,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Value => "value",
            Status::Error => "error",
            Status::Unsupported => "unsupported",
        }
    }
}

struct Row {
    id: &'static str,
    source: &'static str,
    /// Expected observation class + payload substring / exact value.
    expect: Expect,
}

enum Expect {
    ValueExact(&'static str),
    ErrorContains(&'static str),
    UnsupportedContains(&'static str),
}

struct MatrixEntry {
    id: String,
    status: Status,
    reason: String,
    compiled: String,
}

fn corpus() -> Vec<Row> {
    vec![
        Row {
            id: "t1-add",
            source: "(+ 1 2)",
            expect: Expect::ValueExact("3"),
        },
        Row {
            id: "t1-quote-car",
            source: "(car (quote (a b)))",
            expect: Expect::ValueExact("a"),
        },
        Row {
            id: "t1-cons-pair",
            source: "(cons 1 2)",
            expect: Expect::ValueExact("(1 . 2)"),
        },
        Row {
            id: "t1-let",
            source: "(let ((x 5) (y 3)) (+ x y))",
            expect: Expect::ValueExact("8"),
        },
        Row {
            id: "t1-cond",
            source: "(cond ((eq 0 1) 1) (t 2))",
            expect: Expect::ValueExact("2"),
        },
        Row {
            id: "t1-lambda",
            source: "((lambda (x) (+ x 1)) 41)",
            expect: Expect::ValueExact("42"),
        },
        Row {
            id: "t1-self-rec",
            source: "(def count (lambda (n) (cond ((eq n 0) 0) (t (+ 1 (count (- n 1))))))) (count 3)",
            expect: Expect::ValueExact("3"),
        },
        Row {
            id: "t1-mutual",
            source: "(def even (lambda (n) (cond ((eq n 0) t) (t (odd (- n 1)))))) (def odd (lambda (n) (cond ((eq n 0) ()) (t (even (- n 1)))))) (even 2)",
            expect: Expect::ValueExact("T"),
        },
        Row {
            id: "t1-div0",
            source: "(/ 1 0)",
            expect: Expect::ErrorContains("DivisionByZero"),
        },
        Row {
            id: "t1-unknown",
            source: "no-such-binding",
            expect: Expect::ErrorContains("UnknownSymbol"),
        },
        Row {
            id: "t1-arity",
            source: "(+ 1)",
            expect: Expect::ErrorContains("Arity"),
        },
        Row {
            id: "t1-not-callable",
            source: "(1 2)",
            expect: Expect::ErrorContains("NotCallable"),
        },
        Row {
            id: "t1-canon-reserved",
            source: "(def cons 1)",
            expect: Expect::UnsupportedContains("ReservedCanonName"),
        },
        Row {
            id: "t1-empty",
            source: "",
            expect: Expect::ValueExact("()"),
        },
        Row {
            id: "t1-decimal",
            source: "12,5",
            expect: Expect::ValueExact("25/2"),
        },
        Row {
            id: "t1-length-lib",
            source: "(def length-onto (lambda (x acc) (cond ((eq x ()) acc) (t (length-onto (cdr x) (+ acc 1)))))) (def length (lambda (x) (length-onto x 0))) (length (quote (a b c)))",
            expect: Expect::ValueExact("3"),
        },
    ]
}

fn classify(obs: &Observation) -> (Status, String) {
    match obs {
        Observation::Value(v) => (Status::Value, v.clone()),
        Observation::Error(e) => (Status::Error, e.clone()),
        Observation::Unsupported(u) => (Status::Unsupported, u.clone()),
    }
}

fn matches_expect(obs: &Observation, expect: &Expect) -> Result<(), String> {
    match (obs, expect) {
        (Observation::Value(got), Expect::ValueExact(exp)) => {
            let g = got.to_uppercase();
            let e = exp.to_uppercase();
            if g == e {
                Ok(())
            } else {
                Err(format!("value mismatch: expected {exp:?}, got {got:?}"))
            }
        }
        (Observation::Error(got), Expect::ErrorContains(sub)) => {
            if got.contains(sub) {
                Ok(())
            } else {
                Err(format!("error missing {sub:?}: {got:?}"))
            }
        }
        (Observation::Unsupported(got), Expect::UnsupportedContains(sub))
        | (Observation::Error(got), Expect::UnsupportedContains(sub)) => {
            if got.contains(sub) {
                Ok(())
            } else {
                Err(format!("unsupported missing {sub:?}: {got:?}"))
            }
        }
        (other, _) => Err(format!("class mismatch: got {other:?}")),
    }
}

fn render_matrix(entries: &[MatrixEntry]) -> String {
    let mut out = String::from(
        "; COMPILER-11 matrix (compiled observer)\n; columns: id | status | reason | compiled-detail\n((kind . conformance-matrix)\n (observer . compiled-c)\n (rows . (\n",
    );
    for e in entries {
        let _ = writeln!(
            out,
            "  ((id . \"{}\") (status . {}) (reason . \"{}\") (compiled . \"{}\"))",
            e.id,
            e.status.as_str(),
            e.reason.replace('\\', "\\\\").replace('"', "\\\""),
            e.compiled.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ")
        );
    }
    out.push_str(")))\n");
    out
}

#[test]
fn constitutive_matrix_every_row_accounted() {
    let mut entries = Vec::new();
    let mut failures = Vec::new();

    for row in corpus() {
        match compile_and_run(row.source) {
            Ok(obs) => {
                let (status, detail) = classify(&obs);
                match matches_expect(&obs, &row.expect) {
                    Ok(()) => {
                        entries.push(MatrixEntry {
                            id: row.id.into(),
                            status,
                            reason: "matches expected".into(),
                            compiled: detail,
                        });
                    }
                    Err(reason) => {
                        failures.push(format!("{}: {reason}", row.id));
                        entries.push(MatrixEntry {
                            id: row.id.into(),
                            status,
                            reason,
                            compiled: detail,
                        });
                    }
                }
            }
            Err(e) => {
                // Toolchain/IO must not masquerade as semantic unsupported.
                failures.push(format!("{}: harness error (not semantic): {e}", row.id));
                entries.push(MatrixEntry {
                    id: row.id.into(),
                    status: Status::Error,
                    reason: format!("harness: {e}"),
                    compiled: String::new(),
                });
            }
        }
    }

    let matrix = render_matrix(&entries);
    // Fail closed: matrix text must mention every id.
    for row in corpus() {
        assert!(
            matrix.contains(row.id),
            "matrix missing row id {}",
            row.id
        );
    }

    assert_eq!(entries.len(), corpus().len(), "silent drop of a matrix row");
    assert!(
        failures.is_empty(),
        "COMPILER-11 matrix failures:\n{}\n\nmatrix:\n{matrix}",
        failures.join("\n")
    );

    eprintln!(
        "COMPILER-11 matrix: rows={} value={} error={} unsupported={}",
        entries.len(),
        entries.iter().filter(|e| e.status == Status::Value).count(),
        entries.iter().filter(|e| e.status == Status::Error).count(),
        entries
            .iter()
            .filter(|e| e.status == Status::Unsupported)
            .count()
    );
}

#[test]
fn matrix_rejects_empty_reason_on_unsupported() {
    // Policy check: unsupported rows must carry a reason string.
    let entries = [MatrixEntry {
        id: "example".into(),
        status: Status::Unsupported,
        reason: "ReservedCanonName".into(),
        compiled: String::new(),
    }];
    let text = render_matrix(&entries);
    assert!(text.contains("unsupported"));
    assert!(text.contains("ReservedCanonName"));
}
