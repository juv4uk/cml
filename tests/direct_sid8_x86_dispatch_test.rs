use cml::{
    ir::Ir,
    lower,
    parser,
    x86_freestanding::{CompileError, X86FreestandingBackend},
};

#[test]
fn bare_sid8_call_reaches_x86_without_name_or_primop_identity() {
    let expressions = parser::parse("(00000101 (quote (A B)))")
        .expect("exact bare SID8 call must parse");
    let program = lower::lower_program(&expressions).expect("exact bare SID8 call must lower");

    let [Ir::App { func, args }] = program.as_slice() else {
        panic!("direct SID call must remain Ir::App, got {program:?}");
    };
    assert_eq!(func.as_ref(), &Ir::Sid(my_lisp::sid!(00000101)));
    assert_eq!(args.len(), 1);

    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("SID 00000101 must dispatch directly to the x86 CAR mechanism");
    assert!(assembly.contains("call wsm_car"));
}

#[test]
fn bare_sid8_cons_call_uses_exact_sid_dispatch() {
    let expressions = parser::parse("(00000100 (quote A) (quote B))")
        .expect("exact bare SID8 call must parse");
    let program = lower::lower_program(&expressions).expect("exact bare SID8 call must lower");

    let [Ir::App { func, args }] = program.as_slice() else {
        panic!("direct SID call must remain Ir::App, got {program:?}");
    };
    assert_eq!(func.as_ref(), &Ir::Sid(my_lisp::sid!(00000100)));
    assert_eq!(args.len(), 2);

    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("SID 00000100 must dispatch directly to the x86 CONS mechanism");
    assert!(assembly.contains("call wsm_cons"));
}

#[test]
fn unsupported_sid8_call_fails_closed_without_name_fallback() {
    let expressions = parser::parse("(11111111 (quote A))")
        .expect("reserved exact SID8 token must parse");
    let program = lower::lower_program(&expressions).expect("typed SID8 call shape must lower");

    let error = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect_err("unimplemented SID8 must fail closed");
    assert_eq!(
        error,
        CompileError::UnsupportedVariant("unimplemented SID8 call")
    );
}
