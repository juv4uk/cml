use cml::c_backend::CBackend;
use cml::compiler_mechanism::RichCompilerMechanismRef;
use cml::ir::{Ir, PrimOp};
use cml::sens_current_lowering::lower_current_sens_source;

const NUCLEUS: &str = include_str!("../external/sens/lib/compiler-nucleus.lisp");

fn collect_mechanisms(ir: &Ir, out: &mut Vec<RichCompilerMechanismRef>) {
    match ir {
        Ir::Prim { op, args } => {
            match op {
                PrimOp::CompilerMechanism(mechanism)
                | PrimOp::CompilerConditionalExactD1(mechanism) => out.push(*mechanism),
                _ => {}
            }
            for arg in args {
                collect_mechanisms(arg, out);
            }
        }
        Ir::Lambda { body, .. } | Ir::Def { value: body, .. } => collect_mechanisms(body, out),
        Ir::App { func, args } => {
            collect_mechanisms(func, out);
            for arg in args {
                collect_mechanisms(arg, out);
            }
        }
        Ir::Cond { branches } => {
            for (test, body) in branches {
                collect_mechanisms(test, out);
                collect_mechanisms(body, out);
            }
        }
        Ir::CondMatch { branches } => {
            for (test, _, body) in branches {
                collect_mechanisms(test, out);
                collect_mechanisms(body, out);
            }
        }
        Ir::Let { bindings, body } => {
            for (_, value) in bindings {
                collect_mechanisms(value, out);
            }
            collect_mechanisms(body, out);
        }
        Ir::Quote(_)
        | Ir::Int(_)
        | Ir::Float(_)
        | Ir::Rational(_, _)
        | Ir::String(_)
        | Ir::Buffer(_)
        | Ir::Nil
        | Ir::True
        | Ir::Var(_)
        | Ir::Builtin(_)
        | Ir::Sid(_)
        | Ir::MachinePrim { .. }
        | Ir::TailSelfCall { .. } => {}
    }
}

fn assert_no_legacy(ir: &Ir) {
    match ir {
        Ir::Sid(_) | Ir::Builtin(_) => {
            panic!("current SENS source leaked legacy callable IR: {ir:?}")
        }
        Ir::Prim { args, .. } => {
            for arg in args {
                assert_no_legacy(arg);
            }
        }
        Ir::Lambda { body, .. } | Ir::Def { value: body, .. } => assert_no_legacy(body),
        Ir::App { func, args } => {
            assert_no_legacy(func);
            for arg in args {
                assert_no_legacy(arg);
            }
        }
        Ir::Cond { branches } => {
            for (a, b) in branches {
                assert_no_legacy(a);
                assert_no_legacy(b);
            }
        }
        Ir::CondMatch { branches } => {
            for (a, _, b) in branches {
                assert_no_legacy(a);
                assert_no_legacy(b);
            }
        }
        Ir::Let { bindings, body } => {
            for (_, value) in bindings {
                assert_no_legacy(value);
            }
            assert_no_legacy(body);
        }
        Ir::Quote(_)
        | Ir::Int(_)
        | Ir::Float(_)
        | Ir::Rational(_, _)
        | Ir::String(_)
        | Ir::Buffer(_)
        | Ir::Nil
        | Ir::True
        | Ir::Var(_)
        | Ir::MachinePrim { .. }
        | Ir::TailSelfCall { .. } => {}
    }
}

#[test]
fn current_sens_nucleus_reaches_all_nine_roles_without_sid8() {
    let lowered = lower_current_sens_source(NUCLEUS)
        .expect("current SENS compiler nucleus must lower to CML IR");

    assert!(!lowered.ir.is_empty());
    for ir in &lowered.ir {
        assert_no_legacy(ir);
    }

    let mut mechanisms = Vec::new();
    for ir in &lowered.ir {
        collect_mechanisms(ir, &mut mechanisms);
    }

    let expected = [
        RichCompilerMechanismRef::Quote,
        RichCompilerMechanismRef::AtomPredicateD1,
        RichCompilerMechanismRef::SelectorTail,
        RichCompilerMechanismRef::SelectorHead,
        RichCompilerMechanismRef::AtomEqualityD1,
        RichCompilerMechanismRef::ConditionalD1,
        RichCompilerMechanismRef::PairConstruct,
        RichCompilerMechanismRef::Lambda,
        RichCompilerMechanismRef::Define,
    ];

    for role in expected {
        assert!(
            mechanisms.contains(&role),
            "current compiler nucleus did not reach mechanism {}",
            role.as_str()
        );
    }

    assert_eq!(lowered.authority.revision.len(), 40);
    assert_eq!(lowered.authority.authority_sha256.len(), 64);
}

#[test]
fn current_sens_nucleus_compiles_as_executable_c0_c_source() {
    let lowered =
        lower_current_sens_source(NUCLEUS).expect("current SENS compiler nucleus must lower");
    let mut backend = CBackend::new();
    let c = backend
        .compile_program(&lowered.ir)
        .expect("current SENS nucleus must reach the C backend");
    assert!(c.contains("v_atom_predicate("));
    assert!(c.contains("v_eq_predicate("));
    assert!(c.contains("require_predicate_bit("));
    assert!(c.contains("mk_cons("));
    assert!(c.contains("require_tag(_v, TAG_CONS, \"car\")"));
    assert!(c.contains("require_tag(_v, TAG_CONS, \"cdr\")"));
    assert!(c.contains("cml_lambda_"));
    assert!(!c.contains("mk_sid_callable("));
}
