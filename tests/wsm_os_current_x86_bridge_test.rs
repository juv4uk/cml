use cml::compiler_mechanism::RichCompilerMechanismRef;
use cml::ir::{Ir, PrimOp, Quoted};
use cml::x86_freestanding::{CompileError, X86FreestandingBackend};
use std::collections::BTreeSet;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn current_mechanism(mechanism: RichCompilerMechanismRef, args: Vec<Ir>) -> Ir {
    Ir::Prim {
        op: PrimOp::CompilerMechanism(mechanism),
        args,
    }
}

fn sym(name: &str) -> Ir {
    Ir::Quote(Quoted::Sym {
        uppercased: name.to_uppercase(),
        original: name.to_string(),
    })
}

fn assemble_and_undefined_symbols(assembly: &str) -> BTreeSet<String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let base =
        std::env::temp_dir().join(format!("cml-current-wsm-os-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let object = base.with_extension("o");
    fs::write(&source, assembly).expect("write generated assembly");

    let assembled = Command::new("cc")
        .args(["-c", "-x", "assembler"])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("assembler must execute");
    assert!(
        assembled.status.success(),
        "assembler failed: {}\n--- generated assembly ---\n{assembly}",
        String::from_utf8_lossy(&assembled.stderr)
    );

    let nm = Command::new("nm")
        .arg("-u")
        .arg(&object)
        .output()
        .expect("nm must execute");
    assert!(nm.status.success(), "nm failed");

    let _ = fs::remove_file(source);
    let _ = fs::remove_file(object);

    String::from_utf8(nm.stdout)
        .expect("nm output is UTF-8")
        .lines()
        .filter_map(|line| line.split_whitespace().last().map(str::to_string))
        .collect()
}

fn compile_and_run_with_exact_d1_runtime(program: &[Ir], stem: &str) -> std::process::Output {
    let assembly = X86FreestandingBackend::new()
        .compile_program(program)
        .expect("current exact-D1 program must compile");

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-current-d1-{stem}-{}-{nonce}",
        std::process::id()
    ));
    let asm_path = base.with_extension("s");
    let c_path = base.with_extension("c");
    let bin_path = base.with_extension("bin");

    fs::write(&asm_path, &assembly).expect("write generated assembly");
    fs::write(
        &c_path,
        r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define PB0 ((((uint64_t)257) << 3) | 7)
#define PB1 ((((uint64_t)258) << 3) | 7)

extern uint64_t wsm_entry(void *);

uint64_t wsm_predicate_bit_0(void *ctx) {
    (void)ctx;
    return PB0;
}

uint64_t wsm_predicate_bit_1(void *ctx) {
    (void)ctx;
    return PB1;
}

uint64_t wsm_predicate_bit_bits(void *ctx, uint64_t value) {
    (void)ctx;
    if (value == PB0) return 0;
    if (value == PB1) return 1;
    abort();
}

uint64_t wsm_cons(void *ctx, uint64_t car, uint64_t cdr) {
    (void)ctx;
    (void)car;
    (void)cdr;
    return UINT64_C(0x1000);
}

void wsm_fail(void *ctx, uint32_t code, uint64_t a, uint64_t b) {
    (void)ctx;
    (void)code;
    (void)a;
    (void)b;
    abort();
}

int main(void) {
    printf("%llu\n", (unsigned long long)wsm_entry(0));
    return 0;
}
"#,
    )
    .expect("write exact-D1 runtime harness");

    let compile = Command::new("cc")
        .arg(&asm_path)
        .arg(&c_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .expect("cc must execute");
    assert!(
        compile.status.success(),
        "link failed: {}\n--- generated assembly ---\n{assembly}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let output = Command::new(&bin_path)
        .output()
        .expect("exact-D1 witness executable must run");
    let _ = fs::remove_file(asm_path);
    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);
    output
}

fn output_word(output: &std::process::Output) -> u64 {
    assert!(
        output.status.success(),
        "witness failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("witness must print one target word")
}

fn exact_cond(pairs: Vec<(Ir, Ir)>) -> Ir {
    let mut args = Vec::with_capacity(pairs.len() * 2);
    for (test, body) in pairs {
        args.push(test);
        args.push(body);
    }
    Ir::Prim {
        op: PrimOp::CompilerConditionalExactD1(RichCompilerMechanismRef::ConditionalD1),
        args,
    }
}

#[test]
fn verified_structural_current_mechanisms_reach_wsm_runtime_without_sid_adapter() {
    let list = Ir::Quote(Quoted::List(vec![Quoted::Sym {
        uppercased: "A".into(),
        original: "A".into(),
    }]));
    let program = vec![
        current_mechanism(
            RichCompilerMechanismRef::PairConstruct,
            vec![sym("head"), Ir::Nil],
        ),
        current_mechanism(RichCompilerMechanismRef::SelectorHead, vec![list.clone()]),
        current_mechanism(RichCompilerMechanismRef::SelectorTail, vec![list]),
    ];

    let backend = X86FreestandingBackend::new();
    let first = backend
        .compile_program(&program)
        .expect("verified structural current mechanisms must compile");
    let second = backend
        .compile_program(&program)
        .expect("second compile must succeed");

    assert_eq!(first, second, "freestanding assembly must be deterministic");
    assert!(first.contains(".globl wsm_entry"));
    assert!(first.contains("call wsm_cons"));
    assert!(first.contains("call wsm_car"));
    assert!(first.contains("call wsm_cdr"));

    let undefined = assemble_and_undefined_symbols(&first);
    assert_eq!(
        undefined,
        BTreeSet::from([
            "wsm_car".to_string(),
            "wsm_cdr".to_string(),
            "wsm_cons".to_string(),
        ])
    );
    let ratified: BTreeSet<String> = wsm_os_target::RUNTIME_IMPORTS
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    assert!(
        undefined.is_subset(&ratified),
        "current mechanism projection escaped the pinned target ABI: {undefined:?}"
    );
}

#[test]
fn exact_d1_current_mechanisms_use_ratified_v8_carrier_without_sid_adapter() {
    let atom = current_mechanism(RichCompilerMechanismRef::AtomPredicateD1, vec![Ir::Int(7)]);
    let eq = current_mechanism(
        RichCompilerMechanismRef::AtomEqualityD1,
        vec![Ir::Int(7), Ir::Int(7)],
    );
    let cond = exact_cond(vec![(atom.clone(), Ir::Int(11)), (eq.clone(), Ir::Int(12))]);

    let assembly = X86FreestandingBackend::new()
        .compile_program(&[atom, eq, cond])
        .expect("verified exact-D1 mechanisms must compile after target v8 pin");

    assert!(assembly.contains("call wsm_predicate_bit_0"));
    assert!(assembly.contains("call wsm_predicate_bit_1"));
    assert!(assembly.contains("call wsm_predicate_bit_bits"));
    assert!(!assembly.contains("call wsm_atom"));
    assert!(!assembly.contains("call wsm_eq"));

    let undefined = assemble_and_undefined_symbols(&assembly);
    let ratified: BTreeSet<String> = wsm_os_target::RUNTIME_IMPORTS
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    assert!(
        undefined.is_subset(&ratified),
        "exact-D1 projection escaped target ABI v8: {undefined:?}"
    );
}

#[test]
fn current_atom_and_partial_eq_execute_with_distinct_d1_and_empty_carriers() {
    let pair = current_mechanism(
        RichCompilerMechanismRef::PairConstruct,
        vec![Ir::Int(1), Ir::Nil],
    );

    let atom_yes = current_mechanism(RichCompilerMechanismRef::AtomPredicateD1, vec![Ir::Int(1)]);
    let atom_no = current_mechanism(
        RichCompilerMechanismRef::AtomPredicateD1,
        vec![pair.clone()],
    );
    let eq_yes = current_mechanism(
        RichCompilerMechanismRef::AtomEqualityD1,
        vec![Ir::Int(4), Ir::Int(4)],
    );
    let eq_no = current_mechanism(
        RichCompilerMechanismRef::AtomEqualityD1,
        vec![Ir::Int(4), Ir::Int(5)],
    );
    let eq_empty = current_mechanism(
        RichCompilerMechanismRef::AtomEqualityD1,
        vec![pair.clone(), pair],
    );

    assert_eq!(
        output_word(&compile_and_run_with_exact_d1_runtime(
            &[atom_yes],
            "atom-yes"
        )),
        ((258_u64) << 3) | 7
    );
    assert_eq!(
        output_word(&compile_and_run_with_exact_d1_runtime(
            &[atom_no],
            "atom-no"
        )),
        ((257_u64) << 3) | 7
    );
    assert_eq!(
        output_word(&compile_and_run_with_exact_d1_runtime(&[eq_yes], "eq-yes")),
        ((258_u64) << 3) | 7
    );
    assert_eq!(
        output_word(&compile_and_run_with_exact_d1_runtime(&[eq_no], "eq-no")),
        ((257_u64) << 3) | 7
    );
    assert_eq!(
        output_word(&compile_and_run_with_exact_d1_runtime(
            &[eq_empty],
            "eq-empty"
        )),
        wsm_os_target::NIL,
        "partial EQ outside the atom domain must preserve structural EMPTY/no-witness"
    );
}

#[test]
fn current_cond_distinguishes_yes_no_empty_and_rejects_non_d1() {
    let pair = current_mechanism(
        RichCompilerMechanismRef::PairConstruct,
        vec![Ir::Int(1), Ir::Nil],
    );
    let no = current_mechanism(
        RichCompilerMechanismRef::AtomPredicateD1,
        vec![pair.clone()],
    );
    let yes = current_mechanism(RichCompilerMechanismRef::AtomPredicateD1, vec![Ir::Int(1)]);
    let empty = current_mechanism(
        RichCompilerMechanismRef::AtomEqualityD1,
        vec![pair.clone(), pair],
    );

    let selected_after_no = exact_cond(vec![(no.clone(), Ir::Int(10)), (yes.clone(), Ir::Int(42))]);
    assert_eq!(
        output_word(&compile_and_run_with_exact_d1_runtime(
            &[selected_after_no],
            "cond-no-then-yes"
        )),
        wsm_os_target::encode_fixnum(42).expect("42 fits")
    );

    let empty_in_test = exact_cond(vec![
        (empty, Ir::Int(10)),
        (yes, Ir::Int(43)),
    ]);
    let empty_output =
        compile_and_run_with_exact_d1_runtime(&[empty_in_test], "cond-empty-test");
    assert!(
        !empty_output.status.success(),
        "merged SENS #4411: structural EMPTY is not an exact-D1 COND test"
    );

    let no_again = current_mechanism(
        RichCompilerMechanismRef::AtomPredicateD1,
        vec![current_mechanism(
            RichCompilerMechanismRef::PairConstruct,
            vec![Ir::Int(2), Ir::Nil],
        )],
    );
    let exhausted = exact_cond(vec![(no, Ir::Int(10)), (no_again, Ir::Int(11))]);
    assert_eq!(
        output_word(&compile_and_run_with_exact_d1_runtime(
            &[exhausted],
            "cond-exhausted"
        )),
        wsm_os_target::NIL
    );

    for (name, wrong) in [
        ("fixnum-zero", Ir::Int(0)),
        ("symbol-t", Ir::True),
        ("literal-empty", Ir::Nil),
    ] {
        let invalid = exact_cond(vec![(wrong, Ir::Int(99))]);
        let output = compile_and_run_with_exact_d1_runtime(&[invalid], name);
        assert!(
            !output.status.success(),
            "non-D1 current COND input {name} must fail closed"
        );
    }
}

#[test]
fn unverified_generic_prim_remains_rejected() {
    let error = X86FreestandingBackend::new()
        .compile_program(&[Ir::Prim {
            op: PrimOp::Cons,
            args: vec![sym("a"), Ir::Nil],
        }])
        .expect_err("generic Prim must not inherit current-SENS admission");
    assert_eq!(error, CompileError::UnsupportedVariant("Prim"));
}
