use std::fs;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use cml::c_backend::{CBackend, CConditionalMechanism};
use cml::ir::{Ir, Params};

fn gcc_command() -> Command {
    let mut cmd = Command::new("gcc");
    if std::env::var("C_INCLUDE_PATH").is_err()
        && std::path::Path::new("/var/guix/profiles/shared/guix-profile/include").exists()
    {
        cmd.env(
            "C_INCLUDE_PATH",
            "/var/guix/profiles/shared/guix-profile/include",
        );
    }
    cmd
}

fn current_backend() -> CBackend {
    CBackend::new().with_current_d1_conditional()
}

fn compile_c(source: &str, stem: &str) -> Output {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-current-cond-{stem}-{}-{nonce}",
        std::process::id()
    ));
    let source_path = base.with_extension("c");
    let binary_path = base.with_extension("bin");
    fs::write(&source_path, source).unwrap();

    let compile = gcc_command()
        .arg(&source_path)
        .arg("-o")
        .arg(&binary_path)
        .output()
        .unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            source
        );
    }

    let run = Command::new(&binary_path).output().unwrap();
    let _ = fs::remove_file(source_path);
    let _ = fs::remove_file(binary_path);
    run
}

fn runtime_and_functions_with_main(program: &[Ir], main_body: &str) -> String {
    let source = current_backend()
        .compile_program(program)
        .expect("current exact-D1 conditional C source");
    let marker = "\nint main(void) {";
    let end = source
        .find(marker)
        .expect("generated C must contain a main function");
    format!(
        "{}\nint main(void) {{\n    bootstrap_builtins();\n{}\n}}\n",
        &source[..end],
        main_body
    )
}

fn predicate_lambda() -> Ir {
    Ir::Lambda {
        params: Params::Fixed(vec!["P".into(), "Q".into()]),
        body: Box::new(Ir::Cond {
            branches: vec![
                (Ir::Var("P".into()), Ir::Int(11)),
                (Ir::Var("Q".into()), Ir::Int(22)),
            ],
        }),
    }
}

#[test]
fn current_cond_selects_only_exact_d1_and_exhausts_to_structural_empty() {
    let program = [predicate_lambda()];
    let source = runtime_and_functions_with_main(
        &program,
        r#"
    Value *closure = mk_closure(cml_lambda_1, global_env);
    Value *yes = mk_predicate_bit(1);
    Value *no = mk_predicate_bit(0);

    Value *yes_no = v_apply(closure, mk_cons(yes, mk_cons(no, &NIL_V)));
    if (yes_no->tag != TAG_INT || yes_no->u.i != 11) return 10;

    Value *no_yes = v_apply(closure, mk_cons(no, mk_cons(yes, &NIL_V)));
    if (no_yes->tag != TAG_INT || no_yes->u.i != 22) return 11;

    Value *no_no = v_apply(closure, mk_cons(no, mk_cons(no, &NIL_V)));
    if (no_no->tag != TAG_NIL) return 12;

    return 0;
"#,
    );

    assert!(source.contains("if (require_predicate_bit("));
    assert!(!source.contains("if (truthy("));

    let run = compile_c(&source, "yes-no-exhaustion");
    assert!(
        run.status.success(),
        "current COND witness failed: status={:?}, stderr={}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn current_cond_rejects_every_legacy_or_host_truth_carrier() {
    let program = [predicate_lambda()];

    for (value, stem) in [
        ("mk_int(1)", "number-one"),
        ("(&TRUE_V)", "symbol-t"),
        ("mk_cons(mk_int(1), &NIL_V)", "legacy-list-one"),
        ("(&NIL_V)", "nil"),
    ] {
        let source = runtime_and_functions_with_main(
            &program,
            &format!(
                r#"
    Value *closure = mk_closure(cml_lambda_1, global_env);
    Value *test = {value};
    (void)v_apply(closure, mk_cons(test, mk_cons(mk_predicate_bit(0), &NIL_V)));
    return 99;
"#
            ),
        );
        let run = compile_c(&source, stem);
        assert!(!run.status.success(), "{stem} unexpectedly became exact D1");
        assert!(
            String::from_utf8_lossy(&run.stderr).starts_with("Type: current-cond"),
            "{stem}: unexpected stderr: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
}

#[test]
fn compatibility_cond_remains_default_and_isolated() {
    assert_eq!(
        CBackend::new().conditional_mechanism(),
        CConditionalMechanism::CompatibilityTruthiness
    );

    let program = [Ir::Cond {
        branches: vec![(Ir::Int(1), Ir::Int(42))],
    }];
    let source = CBackend::new().compile_program(&program).unwrap();
    assert!(source.contains("if (truthy(mk_int(1)))"));
    assert!(!source.contains("if (require_predicate_bit(mk_int(1)"));

    let run = compile_c(&source, "compatibility-default");
    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "42");
}

#[test]
fn nested_and_recursive_conditionals_lower_through_current_d1_mechanism() {
    let nested = [Ir::Lambda {
        params: Params::Fixed(vec!["P".into()]),
        body: Box::new(Ir::Cond {
            branches: vec![(
                Ir::Var("P".into()),
                Ir::Cond {
                    branches: vec![(Ir::Var("P".into()), Ir::Int(7))],
                },
            )],
        }),
    }];
    let nested_source = current_backend().compile_program(&nested).unwrap();
    assert!(nested_source.matches("require_predicate_bit(").count() >= 3);
    assert!(!nested_source.contains("if (truthy("));

    let recursive = [
        Ir::Def {
            name: "WALK".into(),
            value: Box::new(Ir::Lambda {
                params: Params::Fixed(vec!["P".into()]),
                body: Box::new(Ir::Cond {
                    branches: vec![(
                        Ir::Var("P".into()),
                        Ir::App {
                            func: Box::new(Ir::Var("WALK".into())),
                            args: vec![Ir::Var("P".into())],
                        },
                    )],
                }),
            }),
        },
        Ir::Nil,
    ];
    let recursive_source = current_backend().compile_program(&recursive).unwrap();
    assert!(recursive_source.contains("require_predicate_bit("));
    assert!(recursive_source.contains("env_lookup(env, \"WALK\")"));
    assert!(!recursive_source.contains("if (truthy("));
}
