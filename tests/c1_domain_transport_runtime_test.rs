use cml::c_backend::CBackend;
use cml::ir::Ir;
use sens::{Bija3, Bit3, Bit4, CoreD4, CoreDomainIdentity, Expr, ExprKind, Span};
use std::process::Command;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

fn runtime_prefix() -> String {
    let source = CBackend::new()
        .compile_program(&[Ir::Int(0)])
        .expect("C backend runtime source");
    let marker = "\nint main(void) {";
    source[..source.find(marker).expect("generated main marker")].to_string()
}

fn symbol(name: &str) -> Expr {
    Expr {
        kind: ExprKind::Symbol(Rc::from(name)),
        span: Span::default(),
    }
}

fn canonical_domain_wire() -> Vec<u8> {
    let d3 = CoreDomainIdentity::D3(Bija3::from_word(Bit3::new(0b010).unwrap()));
    let d4 = CoreDomainIdentity::D4(CoreD4::from_word(Bit4::new(0b0010).unwrap()));
    let program = vec![
        Expr {
            kind: ExprKind::DomainCall(d3, Rc::from(vec![symbol("x")].into_boxed_slice())),
            span: Span::default(),
        },
        Expr {
            kind: ExprKind::DomainCall(
                d4,
                Rc::from(
                    vec![
                        Expr {
                            kind: ExprKind::List(Rc::from(vec![symbol("x")].into_boxed_slice())),
                            span: Span::default(),
                        },
                        symbol("x"),
                    ]
                    .into_boxed_slice(),
                ),
            ),
            span: Span::default(),
        },
    ];
    sens::wire_encode_program(&program)
}

#[test]
fn exact_domain_transport_preserves_width_payload_and_shape_without_semantic_dispatch() {
    let wire = canonical_domain_wire();
    let c_wire = wire
        .iter()
        .map(|byte| format!("0x{byte:02x}u"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        r#"{}
int main(int argc, char **argv) {{
    bootstrap_builtins();

    const uint8_t canonical_wire[] = {{{c_wire}}};
    Value *decoded_program =
        decode_sens_program_wire(canonical_wire, sizeof(canonical_wire));
    if (decoded_program->tag != TAG_CONS) return 1;

    Value *first_call = v_car(decoded_program);
    Value *program_tail = v_cdr(decoded_program);
    if (first_call->tag != TAG_CONS || program_tail->tag != TAG_CONS) return 2;
    Value *second_call = v_car(program_tail);
    if (v_cdr(program_tail)->tag != TAG_NIL) return 3;
    if (second_call->tag != TAG_CONS) return 4;

    Value *wire_d3 = v_car(first_call);
    Value *wire_d4 = v_car(second_call);
    if (wire_d3->tag != TAG_DOMAIN_IDENTITY ||
        wire_d3->u.domain_identity.width != 3 ||
        wire_d3->u.domain_identity.packed_bits != 2) return 5;
    if (wire_d4->tag != TAG_DOMAIN_IDENTITY ||
        wire_d4->u.domain_identity.width != 4 ||
        wire_d4->u.domain_identity.packed_bits != 2) return 6;
    if (v_eq_same(wire_d3, wire_d4)) return 7;

    Value *wire_d3_args = v_cdr(first_call);
    if (wire_d3_args->tag != TAG_CONS ||
        v_car(wire_d3_args)->tag != TAG_SYM ||
        strcmp(v_car(wire_d3_args)->u.sym, "x") != 0 ||
        v_cdr(wire_d3_args)->tag != TAG_NIL) return 8;

    if (argc > 1 && strcmp(argv[1], "forbidden-sid") == 0) {{
        const uint8_t legacy_sid_wire[] = {{0x53u, 0x57u, 0x01u, 0x01u, 0x51u}};
        (void)decode_sens_program_wire(legacy_sid_wire, sizeof(legacy_sid_wire));
        return 90;
    }}

    /* Non-palindromic payload is intentional: it detects accidental bit reversal. */
    Value *d3 = mk_domain_identity(3, 4);
    Value *d3_same = mk_domain_identity(3, 4);
    Value *d4_same_payload = mk_domain_identity(4, 4);

    if (d3->tag != TAG_DOMAIN_IDENTITY) return 10;
    if (d3->u.domain_identity.width != 3) return 11;
    if (d3->u.domain_identity.packed_bits != 4) return 12;

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

    int expected[3] = {{1, 0, 0}};
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

    let forbidden = Command::new(&bin_path)
        .arg("forbidden-sid")
        .output()
        .expect("negative legacy-Sid wire witness must execute");
    assert!(
        !forbidden.status.success(),
        "legacy Sid wire unexpectedly entered current C1 program-data"
    );
    assert!(
        String::from_utf8_lossy(&forbidden.stderr).contains("legacy Sid8 is forbidden"),
        "negative wire failed for the wrong reason: {}",
        String::from_utf8_lossy(&forbidden.stderr)
    );

    let _ = std::fs::remove_file(c_path);
    let _ = std::fs::remove_file(bin_path);
}
