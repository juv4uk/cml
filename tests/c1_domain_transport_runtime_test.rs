use cml::c_backend::CBackend;
use cml::ir::Ir;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn runtime_prefix() -> String {
    let source = CBackend::new()
        .compile_program(&[Ir::Int(0)])
        .expect("C backend runtime source");
    let marker = "\nint main(void) {";
    source[..source.find(marker).expect("generated main marker")].to_string()
}

#[test]
fn exact_domain_transport_preserves_width_payload_and_shape_without_semantic_dispatch() {
    let source = format!(
        r#"{}
int main(void) {{
    bootstrap_builtins();

    Value *d3 = mk_domain_identity(3, 2);
    Value *d3_same = mk_domain_identity(3, 2);
    Value *d4_same_payload = mk_domain_identity(4, 2);

    if (d3->tag != TAG_DOMAIN_IDENTITY) return 10;
    if (d3->u.domain_identity.width != 3) return 11;
    if (d3->u.domain_identity.packed_bits != 2) return 12;

    if (!v_eq_same(d3, d3_same)) return 13;
    if (v_eq_same(d3, d4_same_payload)) return 14;
    if (v_equal_p(d3, d4_same_payload)) return 15;

    Value *shape = domain_identity_shape(d3);
    if (shape->tag != TAG_CONS) return 16;
    Value *width = v_car(shape);
    if (width->tag != TAG_INT || width->u.i != 3) return 17;
    Value *bits_cell = v_cdr(shape);
    if (bits_cell->tag != TAG_CONS) return 18;
    Value *bits = v_car(bits_cell);

    int expected[3] = {{0, 1, 0}};
    for (int i = 0; i < 3; ++i) {{
        if (bits->tag != TAG_CONS) return 20 + i;
        Value *bit = v_car(bits);
        if (bit->tag != TAG_PREDICATE_BIT) return 30 + i;
        if ((int)bit->u.predicate_bit != expected[i]) return 40 + i;
        bits = v_cdr(bits);
    }}
    if (bits->tag != TAG_NIL) return 50;

    Value *identity_args = mk_cons(d3, &NIL_V);
    Value *shape_via_builtin = builtin_domain_identity_shape(identity_args, &NIL_V);
    if (!v_equal_p(shape, shape_via_builtin)) return 51;

    Value *non_identity_args = mk_cons(mk_int(2), &NIL_V);
    if (builtin_domain_identity_shape_or_empty(non_identity_args, &NIL_V)->tag != TAG_NIL)
        return 52;

    Value *d4_shape = domain_identity_shape(d4_same_payload);
    if (v_car(d4_shape)->u.i != 4) return 53;

    return 0;
}}
"#,
        runtime_prefix()
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-domain-transport-{}-{nonce}",
        std::process::id()
    ));
    let c_path = base.with_extension("c");
    let bin_path = base.with_extension("bin");
    std::fs::write(&c_path, source).unwrap();

    let compile = Command::new("gcc")
        .args(["-std=c11", "-Wall", "-Wextra"])
        .arg(&c_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .expect("gcc must execute");
    assert!(
        compile.status.success(),
        "domain transport runtime did not compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&bin_path)
        .output()
        .expect("domain transport runtime must execute");
    assert!(
        run.status.success(),
        "domain transport runtime failed with {:?}: {}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = std::fs::remove_file(c_path);
    let _ = std::fs::remove_file(bin_path);
}
