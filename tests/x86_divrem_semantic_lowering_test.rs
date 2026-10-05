//! Source-to-native bounded mod/quotient witnesses for cml#587.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::machine_inst::assemble_program;
use cml::native_baseline::NativeExecutable;
use cml::x86_lir::{LirLowerError, lower_ir_to_lir, lir_to_machine_items};
use cml::{lower, parser};
use sens::{Session, eval_program};

fn parse_lower_one(source: &str) -> cml::ir::Ir {
    let exprs = parser::parse(source).expect("parse source");
    let mut lowered = lower::lower_program(&exprs).expect("lower source");
    assert_eq!(lowered.len(), 1);
    lowered.remove(0)
}

fn native_raw(source: &str) -> u64 {
    let ir = parse_lower_one(source);
    let lir = lower_ir_to_lir(&ir).expect("lower bounded div-rem to x86 LIR");
    let items = lir_to_machine_items(&lir).expect("emit constrained div-rem machine items");
    let bytes = assemble_program(&items).expect("assemble div-rem");
    NativeExecutable::load(&bytes).call()
}

fn oracle_u64(source: &str) -> u64 {
    let mut session = Session::default();
    eval_program(source, &mut session)
        .unwrap_or_else(|error| panic!("oracle failed for {source:?}: {error:?}"))
        .value
        .to_string()
        .parse::<u64>()
        .expect("bounded oracle result must be non-negative integer")
}

#[test]
fn mod_and_quotient_source_lower_through_divrem_and_match_oracle() {
    for (source, expected) in [("(mod 20 6)", 2_u64), ("(quotient 20 6)", 3_u64)] {
        let native = native_raw(source);
        let oracle = oracle_u64(source);
        assert_eq!(native, expected);
        assert_eq!(native, oracle, "native result must equal upstream oracle");
    }
}

#[test]
fn lowered_dump_uses_backend_local_divrem_without_shared_register_hints() {
    for source in ["(mod 20 6)", "(quotient 20 6)"] {
        let ir = parse_lower_one(source);
        let lir = lower_ir_to_lir(&ir).expect("bounded source lowers");
        let dump = lir.dump();
        assert!(dump.contains("divrem low="));
        assert!(!format!("{ir:?}").contains("Rax"));
        assert!(!format!("{ir:?}").contains("Rdx"));
    }
}

#[test]
fn bounded_divrem_rejects_unsupported_rows_fail_closed() {
    for source in [
        "(mod -1 3)",
        "(quotient -1 3)",
        "(mod 7 0)",
        "(quotient 7 0)",
        "(mod 7 -3)",
        "(quotient 7 -3)",
    ] {
        let ir = parse_lower_one(source);
        let error = lower_ir_to_lir(&ir).expect_err("row must remain outside bounded x86 divide slice");
        assert!(
            matches!(error, LirLowerError::Unsupported(_)),
            "unexpected error for {source}: {error:?}"
        );
    }
}

#[test]
fn bounded_divrem_rejects_wrong_arity_before_machine_lowering() {
    for source in ["(mod 7)", "(quotient 7)", "(mod 7 3 1)", "(quotient 7 3 1)"] {
        let ir = parse_lower_one(source);
        let error = lower_ir_to_lir(&ir).expect_err("wrong arity must fail closed");
        assert!(
            matches!(
                error,
                LirLowerError::InvalidArity {
                    expected: 2,
                    ..
                }
            ),
            "unexpected error for {source}: {error:?}"
        );
    }
}
