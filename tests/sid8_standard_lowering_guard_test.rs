use cml::ir::Ir;
use cml::{lower, parser};

fn assert_no_legacy_callable_identity(ir: &Ir) {
    match ir {
        Ir::Builtin(name) => {
            panic!("standard lowering leaked Builtin callable identity: {name}")
        }
        Ir::Prim { op, .. } => {
            panic!("standard lowering leaked Prim callable identity: {op:?}")
        }
        Ir::Lambda { body, .. } => assert_no_legacy_callable_identity(body),
        Ir::App { func, args } => {
            assert_no_legacy_callable_identity(func);
            for arg in args {
                assert_no_legacy_callable_identity(arg);
            }
        }
        Ir::Cond { branches } => {
            for (test, body) in branches {
                assert_no_legacy_callable_identity(test);
                assert_no_legacy_callable_identity(body);
            }
        }
        Ir::CondMatch { branches } => {
            for (query, _, body) in branches {
                assert_no_legacy_callable_identity(query);
                assert_no_legacy_callable_identity(body);
            }
        }
        Ir::Let { bindings, body } => {
            for (_, value) in bindings {
                assert_no_legacy_callable_identity(value);
            }
            assert_no_legacy_callable_identity(body);
        }
        Ir::Def { value, .. } => assert_no_legacy_callable_identity(value),
        Ir::MachinePrim { args, .. } | Ir::TailSelfCall { args } => {
            for arg in args {
                assert_no_legacy_callable_identity(arg);
            }
        }
        Ir::Sid(_)
        | Ir::Int(_)
        | Ir::Float(_)
        | Ir::Rational(_, _)
        | Ir::String(_)
        | Ir::Buffer(_)
        | Ir::Nil
        | Ir::True
        | Ir::Var(_)
        | Ir::Quote(_) => {}
    }
}

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).expect("fixture must parse");
    let mut program = lower::lower_program(&expressions).expect("fixture must lower");
    assert_eq!(program.len(), 1, "fixture must lower to one top-level node");
    program.remove(0)
}

#[test]
fn standard_lowering_never_reifies_language_callables_as_builtin_or_prim() {
    let fixtures = [
        "(atom 1)",
        "(eq 1 1)",
        "(cons 1 (quote ()))",
        "(car (quote (1 2)))",
        "(cdr (quote (1 2)))",
        "(+ 1 2)",
        "(- 3 1)",
        "(equal? (quote (1 2)) (quote (1 2)))",
        "(list 1 2 3)",
        "(mod 7 3)",
        "(quotient 8 2)",
        "(<= 1 2)",
        "(>= 2 1)",
        "(lambda (x) (+ x 1))",
        "(let ((x 1)) (+ x 2))",
    ];

    for source in fixtures {
        let ir = lower_one(source);
        assert_no_legacy_callable_identity(&ir);
    }
}

#[test]
fn admitted_first_class_callable_values_are_exact_sid8() {
    for source in ["atom", "eq", "cons", "car", "cdr", "+", "-", "equal?", "list"] {
        let ir = lower_one(source);
        assert!(
            matches!(ir, Ir::Sid(_)),
            "first-class callable {source:?} must lower to exact Sid8, got {ir:?}"
        );
    }
}
