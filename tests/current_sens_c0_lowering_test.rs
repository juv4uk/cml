use cml::c_backend::CBackend;
use cml::compiler_mechanism::RichCompilerMechanismRef;
use cml::ir::{Ir, PrimOp};
use cml::sens_current_lowering::lower_current_sens_source;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

const NUCLEUS: &str = include_str!("../external/sens/lib/compiler-nucleus.lisp");

fn pinned_compiler_export() -> &'static str {
    static EXPORT: OnceLock<String> = OnceLock::new();
    EXPORT
        .get_or_init(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let manifest = root.join("external/sens/Cargo.toml");
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos();
            let target = std::env::temp_dir().join(format!(
                "cml-current-ir-sens-export-{}-{nonce}",
                std::process::id()
            ));

            let output = Command::new("cargo")
                .current_dir(root.join("external/sens"))
                .env("CARGO_TARGET_DIR", &target)
                .args([
                    "run",
                    "--quiet",
                    "--manifest-path",
                    manifest.to_str().expect("UTF-8 SENS manifest path"),
                    "-p",
                    "xtask",
                    "--",
                    "compiler-export",
                ])
                .output()
                .expect("pinned SENS compiler-export must execute");

            let _ = std::fs::remove_dir_all(&target);
            assert!(
                output.status.success(),
                "pinned SENS compiler-export failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).expect("compiler export is UTF-8")
        })
        .as_str()
}

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
fn real_export_drives_full_current_closure_without_sid8() {
    let lowered = lower_current_sens_source(NUCLEUS, pinned_compiler_export())
        .expect("real verified SENS export must lower current compiler nucleus");

    assert!(!lowered.ir.is_empty());
    for ir in &lowered.ir {
        assert_no_legacy(ir);
    }

    let mut mechanisms = Vec::new();
    for ir in &lowered.ir {
        collect_mechanisms(ir, &mut mechanisms);
    }

    for expected in [
        RichCompilerMechanismRef::AtomPredicateD1,
        RichCompilerMechanismRef::SelectorTail,
        RichCompilerMechanismRef::SelectorHead,
        RichCompilerMechanismRef::AtomEqualityD1,
        RichCompilerMechanismRef::ConditionalD1,
        RichCompilerMechanismRef::PairConstruct,
    ] {
        assert!(
            mechanisms.contains(&expected),
            "current compiler nucleus did not reach mechanism-bearing IR {}",
            expected.as_str()
        );
    }

    fn syntax_witness(ir: &Ir, quote: &mut bool, lambda: &mut bool, define: &mut bool) {
        match ir {
            Ir::Quote(_) => *quote = true,
            Ir::Lambda { body, .. } => {
                *lambda = true;
                syntax_witness(body, quote, lambda, define);
            }
            Ir::Def { value, .. } => {
                *define = true;
                syntax_witness(value, quote, lambda, define);
            }
            Ir::Prim { args, .. } => {
                for arg in args {
                    syntax_witness(arg, quote, lambda, define);
                }
            }
            Ir::App { func, args } => {
                syntax_witness(func, quote, lambda, define);
                for arg in args {
                    syntax_witness(arg, quote, lambda, define);
                }
            }
            Ir::Cond { branches } => {
                for (a, b) in branches {
                    syntax_witness(a, quote, lambda, define);
                    syntax_witness(b, quote, lambda, define);
                }
            }
            Ir::CondMatch { branches } => {
                for (a, _, b) in branches {
                    syntax_witness(a, quote, lambda, define);
                    syntax_witness(b, quote, lambda, define);
                }
            }
            Ir::Let { bindings, body } => {
                for (_, value) in bindings {
                    syntax_witness(value, quote, lambda, define);
                }
                syntax_witness(body, quote, lambda, define);
            }
            _ => {}
        }
    }

    let (mut quote, mut lambda, mut define) = (false, false, false);
    for ir in &lowered.ir {
        syntax_witness(ir, &mut quote, &mut lambda, &mut define);
    }
    assert!(
        quote && lambda && define,
        "verified Quote/Lambda/Define mechanisms must materialize as existing syntax IR"
    );

    assert_eq!(
        lowered.authority.revision,
        "c66d4743bb70882c75376dbcec27d393e5a9649d"
    );
    assert_eq!(lowered.authority.authority_sha256.len(), 64);
}

#[test]
fn real_export_nucleus_compiles_as_current_c0_c_source() {
    let lowered = lower_current_sens_source(NUCLEUS, pinned_compiler_export())
        .expect("real verified SENS export must lower");
    let mut backend = CBackend::new();
    let c = backend
        .compile_program(&lowered.ir)
        .expect("current SENS nucleus must reach C backend");

    assert!(c.contains("v_atom_predicate("));
    assert!(c.contains("v_eq_predicate("));
    assert!(c.contains("require_predicate_bit("));
    assert!(c.contains("mk_cons("));
    assert!(c.contains("require_tag(_v, TAG_CONS, \"car\")"));
    assert!(c.contains("require_tag(_v, TAG_CONS, \"cdr\")"));
    assert!(c.contains("cml_lambda_"));
    assert!(
        !c.contains("mk_sid_callable(0b"),
        "current C0 must not emit a Sid8 callable invocation"
    );
}
