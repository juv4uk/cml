use cml::{
    c_backend::{CBackend, CompileError},
    ir::Ir,
    lower, parser,
};

#[test]
fn bare_sid8_car_call_reaches_c_backend_without_name_or_primop_identity() {
    let expressions =
        parser::parse("(00000101 (quote (A B)))").expect("exact bare SID8 call must parse");
    let program = lower::lower_program(&expressions).expect("exact bare SID8 call must lower");

    let [Ir::App { func, args }] = program.as_slice() else {
        panic!("direct SID call must remain Ir::App, got {program:?}");
    };
    assert_eq!(func.as_ref(), &Ir::Sid(my_lisp::sid!(00000101)));
    assert_eq!(args.len(), 1);

    let c = CBackend::new()
        .compile_program(&program)
        .expect("SID 00000101 must dispatch directly to the C CAR mechanism");

    assert!(c.contains("v_car("));
    assert!(!c.contains("env_lookup(&NIL_V, \"CAR\")"));
}

#[test]
fn bare_sid8_cons_call_uses_exact_sid_dispatch_in_c_backend() {
    let expressions =
        parser::parse("(00000100 (quote A) (quote B))").expect("exact bare SID8 call must parse");
    let program = lower::lower_program(&expressions).expect("exact bare SID8 call must lower");

    let [Ir::App { func, args }] = program.as_slice() else {
        panic!("direct SID call must remain Ir::App, got {program:?}");
    };
    assert_eq!(func.as_ref(), &Ir::Sid(my_lisp::sid!(00000100)));
    assert_eq!(args.len(), 2);

    let c = CBackend::new()
        .compile_program(&program)
        .expect("SID 00000100 must dispatch directly to the C CONS mechanism");

    assert!(c.contains("mk_cons("));
    assert!(!c.contains("env_lookup(&NIL_V, \"CONS\")"));
}

#[test]
fn unsupported_sid8_call_fails_closed_in_c_backend_without_name_fallback() {
    let expressions =
        parser::parse("(11111111 (quote A))").expect("reserved exact SID8 token must parse");
    let program = lower::lower_program(&expressions).expect("typed SID8 call shape must lower");

    let error = CBackend::new()
        .compile_program(&program)
        .expect_err("unimplemented SID8 must fail closed");

    assert!(
        matches!(
            error,
            CompileError::UnsupportedVariant("unimplemented SID8 call")
        ),
        "unexpected C-backend SID8 failure: {error}"
    );
}
