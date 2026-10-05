//! First bounded differential preservation witness for cml#453.
//!
//! Expected values are not duplicated in CML.  The pinned SENS dependency
//! evaluates the same source and supplies the observable; CML independently
//! lowers, emits C, compiles and executes it.

use cml::build::{Observation, compile_and_run};

fn sens_oracle(source: &str) -> Observation {
    let mut session = sens::Session::default();
    let result = sens::eval_program(source, &mut session)
        .unwrap_or_else(|error| panic!("pinned SENS oracle rejected {source:?}: {error}"));
    Observation::Value(result.value.to_string())
}

#[test]
fn ast_to_ir_compiled_path_matches_pinned_sens_on_first_bounded_slice() {
    // Deliberately tiny and explicit.  This is a bounded differential slice,
    // not a claim that the unbounded program space has been exhausted.
    const SOURCES: &[&str] = &[
        "(+ 1 2)",
        "(+ (+ 1 2) 3)",
        "(- 7 2)",
        "(cons 1 2)",
        "(car (cons 7 8))",
        "(cdr (cons 7 8))",
    ];

    for source in SOURCES {
        let expected = sens_oracle(source);
        let actual = compile_and_run(source)
            .unwrap_or_else(|error| panic!("CML compiled path failed for {source:?}: {error}"));
        assert_eq!(
            actual, expected,
            "bounded differential mismatch for source {source:?}"
        );
    }
}
