use cml::ir::Ir;
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).expect("source must parse");
    let mut lowered = lower::lower_program(&expressions).expect("source must lower");
    assert_eq!(
        lowered.len(),
        1,
        "fixture must contain exactly one expression"
    );
    lowered.remove(0)
}

fn assert_sid_call(source: &str, expected_sid: sens::Sid8) {
    let lowered = lower_one(source);
    match lowered {
        Ir::App { func, .. } => match *func {
            Ir::Sid(actual) => assert_eq!(
                actual, expected_sid,
                "function call must preserve exact Sid8 after surface resolution"
            ),
            other => {
                panic!("function call identity must be Ir::Sid({expected_sid}), got {other:?}")
            }
        },
        other => panic!("function call must lower to Ir::App keyed by Sid8, got {other:?}"),
    }
}

#[test]
fn admitted_surface_calls_keep_sid8_as_the_ir_function_key() {
    assert_sid_call("(car (quote (A B)))", sens::sid!(00000101));
    assert_sid_call("(cons (quote A) (quote B))", sens::sid!(00000100));
    assert_sid_call(
        "(list (quote A) (quote B) (quote C))",
        sens::sid!(00100111),
    );
    assert_sid_call("(+ 1 2)", sens::sid!(00001100));
}

#[test]
fn peer_surface_calls_converge_to_the_same_sid8_before_backend_entry() {
    assert_sid_call("(перше (quote (A B)))", sens::sid!(00000101));
    assert_sid_call("(сполучити (quote A) (quote B))", sens::sid!(00000100));
    assert_sid_call("(додати 1 2)", sens::sid!(00001100));
}

#[test]
fn first_class_callable_surface_is_sid8_not_builtin_name_or_primop_identity() {
    let lowered = lower_one("car");
    assert_eq!(
        lowered,
        Ir::Sid(sens::sid!(00000101)),
        "first-class callable identity must remain exact Sid8"
    );
}

#[test]
fn exact_bare_sid8_call_already_uses_the_required_ir_shape() {
    assert_sid_call("(00000101 (quote (A B)))", sens::sid!(00000101));
}
