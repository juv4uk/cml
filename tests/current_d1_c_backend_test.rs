use std::fs;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use cml::c_backend::CBackend;
use cml::ir::Ir;

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

fn runtime_prefix() -> String {
    let source = CBackend::new()
        .compile_program(&[Ir::Int(0)])
        .expect("C backend runtime source");
    let marker = "\nint main(void) {";
    let end = source
        .find(marker)
        .expect("generated C must contain its main function");
    source[..end].to_string()
}

fn compile_and_run(body: &str, stem: &str) -> Output {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-current-d1-{stem}-{}-{nonce}",
        std::process::id()
    ));
    let source_path = base.with_extension("c");
    let binary_path = base.with_extension("bin");

    let source = format!(
        r#"{}

static Value *d1_echo_first(Value *args, Value *env) {{
    (void)env;
    require_arity(args, 1, "d1-echo");
    return arg_at(args, 0);
}}

int main(void) {{
{}
}}
"#,
        runtime_prefix(),
        body
    );
    fs::write(&source_path, &source).unwrap();

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

#[test]
fn current_d1_is_a_distinct_one_bit_value_and_survives_argument_passing() {
    let source = runtime_prefix();
    assert!(source.contains("TAG_PREDICATE_BIT"));
    assert!(source.contains("static Value *v_atom_predicate"));
    assert!(source.contains("static Value *v_eq_predicate"));
    assert!(source.contains("static int require_predicate_bit"));

    // Compatibility builtins remain on their historical result carrier.
    assert!(source.contains(
        "static Value *builtin_atom(Value *args, Value *env) { (void)env; require_arity(args, 1, \"atom\"); return v_structural_kind(arg_at(args, 0)); }"
    ));
    assert!(source.contains("return v_eq(left, right);"));

    let run = compile_and_run(
        r#"
    Value *yes = mk_predicate_bit(1);
    Value *no = mk_predicate_bit(0);
    if (yes->tag != TAG_PREDICATE_BIT || yes->u.predicate_bit != 1) return 10;
    if (no->tag != TAG_PREDICATE_BIT || no->u.predicate_bit != 0) return 11;

    Value *zero = mk_int(0);
    Value *one = mk_int(1);
    Value *list_zero = mk_cons(zero, &NIL_V);
    Value *list_one = mk_cons(one, &NIL_V);
    if (yes->tag == TAG_NIL || yes->tag == one->tag || yes->tag == list_one->tag) return 12;
    if (no->tag == TAG_NIL || no->tag == zero->tag || no->tag == list_zero->tag) return 13;

    Value *atom_yes = v_atom_predicate(&NIL_V);
    Value *atom_no = v_atom_predicate(mk_cons(mk_int(1), &NIL_V));
    if (require_predicate_bit(atom_yes, "atom") != 1) return 14;
    if (require_predicate_bit(atom_no, "atom") != 0) return 15;

    Value *x1 = mk_sym("X");
    Value *x2 = mk_sym("X");
    Value *y = mk_sym("Y");
    if (require_predicate_bit(v_eq_predicate(x1, x2), "eq") != 1) return 16;
    if (require_predicate_bit(v_eq_predicate(x1, y), "eq") != 0) return 17;

    Value *closure = mk_closure(d1_echo_first, &NIL_V);
    Value *echoed = v_apply(closure, mk_cons(yes, &NIL_V));
    if (echoed->tag != TAG_PREDICATE_BIT || echoed->u.predicate_bit != 1) return 18;

    return 0;
"#,
        "carrier",
    );

    assert!(
        run.status.success(),
        "D1 carrier witness failed: status={:?}, stderr={}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn current_eq_rejects_pairs_instead_of_manufacturing_d1() {
    let run = compile_and_run(
        r#"
    Value *pair = mk_cons(mk_int(1), &NIL_V);
    (void)v_eq_predicate(pair, pair);
    return 99;
"#,
        "eq-pair-reject",
    );
    assert!(!run.status.success());
    assert!(
        String::from_utf8_lossy(&run.stderr).starts_with("Type: eq"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn d1_consumer_rejects_numeric_and_legacy_list_truth_carriers() {
    for (body, stem) in [
        (
            r#"
    (void)require_predicate_bit(mk_int(1), "current-cond");
    return 99;
"#,
            "numeric-reject",
        ),
        (
            r#"
    (void)require_predicate_bit(mk_cons(mk_int(1), &NIL_V), "current-cond");
    return 99;
"#,
            "legacy-list-reject",
        ),
    ] {
        let run = compile_and_run(body, stem);
        assert!(!run.status.success());
        assert!(
            String::from_utf8_lossy(&run.stderr).starts_with("Type: current-cond"),
            "{stem}: unexpected stderr: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
}
