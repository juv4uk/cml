use cml::ir::Ir;
use cml::{lower, parser};

fn lower_first_class(source: &str) -> Ir {
    let parsed = parser::parse(source).expect("source must parse");
    let mut lowered =
        lower::lower_program_with_first_class_builtins(&parsed).expect("C frontend must lower");
    assert_eq!(lowered.len(), 1);
    lowered.remove(0)
}

#[test]
fn first_class_callable_identity_stays_exact_sid8() {
    assert!(
        matches!(lower_first_class("car"), Ir::Sid(sid) if sid == my_lisp::sid!(00000101)),
        "C frontend must not project first-class CAR back to a name"
    );
}

#[test]
fn direct_callable_application_stays_exact_sid8() {
    let ir = lower_first_class("(car (quote (1 2)))");
    assert!(
        matches!(ir, Ir::App { ref func, .. }
            if matches!(func.as_ref(), Ir::Sid(sid) if *sid == my_lisp::sid!(00000101))),
        "C frontend must preserve exact CAR Sid8 in call position: {ir:?}"
    );
}

#[test]
fn higher_order_numeric_buffer_map_keeps_nested_sid8_callable() {
    let ir = lower_first_class("(numeric-buffer-map + #i32(1 2 3))");
    let Ir::App { func, args } = ir else {
        panic!("expected numeric-buffer-map application");
    };
    assert!(matches!(func.as_ref(), Ir::Sid(sid) if *sid == my_lisp::sid!(01011001)));
    assert!(
        matches!(args.first(), Some(Ir::Sid(sid)) if *sid == my_lisp::sid!(00001100)),
        "higher-order + must remain exact Sid8, got {args:?}"
    );
}
