use cml::canon::{
    CANON_OPERATIONS_TABLE, callable_semantic_id, find_operation_by_id,
    find_operation_by_surface,
};
use cml::ir::{Ir, PrimOp};
use cml::{lower, parser};
use my_lisp::Sid8;

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).expect("source must parse");
    let mut lowered = lower::lower_program(&expressions).expect("source must lower");
    assert_eq!(lowered.len(), 1, "fixture must contain one expression");
    lowered.remove(0)
}

#[test]
fn list_sid_is_generated_as_a_callable_and_lowers_through_sid_identity() {
    assert_eq!(
        callable_semantic_id("list"),
        Some(my_lisp::sid!(00100111)),
        "LIST must be admitted from the upstream registry, not a spelling special case"
    );
    assert_eq!(
        callable_semantic_id("LIST"),
        Some(my_lisp::sid!(00100111)),
        "Latin callable lookup remains case-folded"
    );

    let lowered = lower_one("(list (quote A) (quote B) (quote C))");
    assert!(
        matches!(lowered, Ir::Prim { op: PrimOp::List, ref args } if args.len() == 3),
        "LIST call must reach the existing PrimOp::List mechanism; got {lowered:?}"
    );

    let empty = lower_one("(list)");
    assert!(
        matches!(empty, Ir::Prim { op: PrimOp::List, ref args } if args.is_empty()),
        "zero-arity LIST must remain the same variadic primitive mechanism; got {empty:?}"
    );
}

#[test]
fn every_admitted_canon_callable_surface_lowers_to_one_semantic_operation() {
    // Registry-driven Canon dispatch:
    // These peer spellings are the stable surfaces of semantic IDs 0002..0006, 0104, 1001, 1022
    // in my-lisp/lib/surface/semantic-registry.wsm.
    let cases: &[(my_lisp::Sid8, &str, &str, &[&str])] = &[
        (
            my_lisp::sid!(00000010),
            "atom",
            "1",
            &["атом?", "aṇu", ".?"],
        ),
        (
            my_lisp::sid!(00000011),
            "eq",
            "1 1",
            &["тотожне?", "abheda", "=?"],
        ),
        (
            my_lisp::sid!(00000100),
            "cons",
            "1 (quote ())",
            &["сполучити", "saṃyuj", ":"],
        ),
        (
            my_lisp::sid!(00000101),
            "car",
            "(quote (1 2))",
            &["перше", "ādi", ":п"],
        ),
        (
            my_lisp::sid!(00000110),
            "cdr",
            "(quote (1 2))",
            &["решта", "śeṣa", ":р"],
        ),
        (my_lisp::sid!(00001100), "+", "1 2", &["додати", "yoga"]),
        (my_lisp::sid!(00001101), "-", "3 1", &["відняти", "viyoga"]),
        (
            my_lisp::sid!(00100010),
            "equal?",
            "(quote (1 2)) (quote (1 2))",
            &["однакові?", "tulya?"],
        ),
    ];

    for (semantic_id, english, args, peers) in cases {
        let baseline = lower_one(&format!("({english} {args})"));
        for peer in *peers {
            let actual = lower_one(&format!("({peer} {args})"));
            assert_eq!(
                actual, baseline,
                "surface {peer:?} must lower through semantic identity {semantic_id}, not through spelling-specific dispatch"
            );
        }
    }
}

#[test]
fn every_admitted_canon_callable_surface_is_the_same_first_class_value() {
    // Call-position identity is not enough: Canon callables are first-class.
    // A peer surface used as a value must therefore lower to the same builtin
    // identity as its English peer instead of becoming a spelling-named Var.
    let cases: &[(my_lisp::Sid8, &str, &[&str])] = &[
        (my_lisp::sid!(00000010), "atom", &["атом?", "aṇu", ".?"]),
        (my_lisp::sid!(00000011), "eq", &["тотожне?", "abheda", "=?"]),
        (
            my_lisp::sid!(00000100),
            "cons",
            &["сполучити", "saṃyuj", ":"],
        ),
        (my_lisp::sid!(00000101), "car", &["перше", "ādi", ":п"]),
        (my_lisp::sid!(00000110), "cdr", &["решта", "śeṣa", ":р"]),
        (my_lisp::sid!(00001100), "+", &["додати", "yoga"]),
        (my_lisp::sid!(00001101), "-", &["відняти", "viyoga"]),
        (my_lisp::sid!(00100010), "equal?", &["однакові?", "tulya?"]),
    ];

    for (semantic_id, english, peers) in cases {
        let baseline = lower_one(english);
        for peer in *peers {
            let actual = lower_one(peer);
            assert_eq!(
                actual, baseline,
                "first-class surface {peer:?} must denote semantic identity {semantic_id}, not a spelling-specific variable"
            );
        }
    }
}

#[test]
fn canon_operations_table_is_fully_populated_and_queryable() {
    assert!(
        CANON_OPERATIONS_TABLE.len() >= 15,
        "operations table must contain all admitted operations"
    );

    // Verify lookup by ID and by surface
    let add_op = find_operation_by_id(my_lisp::sid!(00001100))
        .expect("0104 (+) must exist in operations table");
    assert_eq!(add_op.canonical_name, "+");
    assert_eq!(add_op.formal_action, "primitive:add");
    assert_eq!(
        find_operation_by_surface("додати"),
        Some(add_op),
        "Ukrainian surface додати must resolve to 0104"
    );
    assert_eq!(
        find_operation_by_surface("yoga"),
        Some(add_op),
        "Sanskrit surface yoga must resolve to 0104"
    );
    assert_eq!(
        find_operation_by_surface("+"),
        Some(add_op),
        "Symbol surface + must resolve to 0104"
    );

    let equal_op = find_operation_by_id(my_lisp::sid!(00100010))
        .expect("1022 (equal?) must exist in operations table");
    assert_eq!(equal_op.canonical_name, "equal?");
    assert_eq!(
        find_operation_by_surface("однакові?"),
        Some(equal_op),
        "Ukrainian surface однакові? must resolve to 1022"
    );
}

#[test]
fn backend_lowering_parity_across_backends_for_ukrainian_surfaces() {
    use cml::c_backend::CBackend;
    use cml::compiler::Compiler;
    use cml::x86_freestanding::X86FreestandingBackend;

    // x86 parity: (+ 1 2) vs (додати 1 2)
    let x86 = X86FreestandingBackend::new();
    let ir_en = lower::lower_program(&parser::parse("(+ 1 2)").unwrap()).unwrap();
    let ir_uk = lower::lower_program(&parser::parse("(додати 1 2)").unwrap()).unwrap();
    let asm_en = x86.compile_program(&ir_en).unwrap();
    let asm_uk = x86.compile_program(&ir_uk).unwrap();
    assert_eq!(
        asm_en, asm_uk,
        "x86 backend must emit identical assembly for peer surfaces"
    );

    // C backend parity: (- 5 3) vs (відняти 5 3)
    let mut c_backend = CBackend::new();
    let ir_sub_en = lower::lower_program(&parser::parse("(- 5 3)").unwrap()).unwrap();
    let ir_sub_uk = lower::lower_program(&parser::parse("(відняти 5 3)").unwrap()).unwrap();
    let c_en = c_backend.compile_program(&ir_sub_en).unwrap();
    let c_uk = c_backend.compile_program(&ir_sub_uk).unwrap();
    assert_eq!(
        c_en, c_uk,
        "C backend must emit identical code for peer surfaces"
    );

    // FPGA compiler parity: (cons 1 2) vs (сполучити 1 2)
    let mut fpga = Compiler::new();
    let ir_cons_en = lower::lower_program(&parser::parse("(cons 1 2)").unwrap()).unwrap();
    let ir_cons_uk = lower::lower_program(&parser::parse("(сполучити 1 2)").unwrap()).unwrap();
    let fpga_en = fpga.compile(&ir_cons_en).unwrap();
    let mut fpga_uk_compiler = Compiler::new();
    let fpga_uk = fpga_uk_compiler.compile(&ir_cons_uk).unwrap();
    assert_eq!(
        fpga_en, fpga_uk,
        "FPGA compiler must emit identical instructions for peer surfaces"
    );
}

#[test]
fn operations_table_file_matches_compiled_canon_table() {
    let ops_path = if std::path::Path::new("contracts/cml-operations.lisp").exists() {
        "contracts/cml-operations.lisp"
    } else {
        "contracts/cml-operations.my"
    };
    let file_content =
        std::fs::read_to_string(ops_path).expect("contracts/cml-operations contract must exist");
    assert!(file_content.contains("((kind . cml-operations-table)"));
    assert!(file_content.contains("(version . (1 0))"));

    // Ensure every compiled operation ID is present in the machine-readable file
    for op in CANON_OPERATIONS_TABLE {
        assert!(
            file_content.contains(&format!("(semantic-id . {:?})", op.semantic_id)),
            "operations table file must contain semantic-id {}",
            op.semantic_id
        );
        assert!(
            file_content.contains(&format!("(canonical-name . {:?})", op.canonical_name)),
            "operations table file must contain canonical-name {}",
            op.canonical_name
        );
    }
}

#[test]
fn lookup_policy_vs_quoted_data_identity_separation() {
    use cml::ir::Quoted;
    // Data identity: exact casing preserved for quoted symbols (radio != RADIO)
    let program = lower_one("(quote (radio RADIO))");
    match program {
        Ir::Quote(Quoted::List(items)) => {
            assert_eq!(items.len(), 2);
            match (&items[0], &items[1]) {
                (
                    Quoted::Sym {
                        original: orig1, ..
                    },
                    Quoted::Sym {
                        original: orig2, ..
                    },
                ) => {
                    assert_eq!(orig1, "radio");
                    assert_eq!(orig2, "RADIO");
                    assert_ne!(
                        orig1, orig2,
                        "quoted data identity must preserve exact casing: radio != RADIO"
                    );
                }
                other => panic!("expected Quoted::Sym pair, got {other:?}"),
            }
        }
        other => panic!("expected Ir::Quote(Quoted::List), got {other:?}"),
    }

    // Name lookup policy: uppercase case-folding for Latin builtins, exact Unicode for Ukrainian
    let lower_en = lower_one("car");
    let lower_en_upper = lower_one("CAR");
    assert_eq!(lower_en, lower_en_upper);

    let lower_uk = lower_one("перше");
    assert_eq!(lower_en, lower_uk);
}

#[test]
fn unknown_canon_operations_fail_closed() {
    assert_eq!(find_operation_by_id(my_lisp::sid!(11111111)), None);
    assert_eq!(find_operation_by_surface("nonexistent-function-xyz"), None);
    assert_eq!(
        cml::canon::canonical_builtin_name(my_lisp::sid!(11111111)),
        None
    );
    assert_eq!(cml::canon::callable_semantic_id("unknown-op"), None);
}
