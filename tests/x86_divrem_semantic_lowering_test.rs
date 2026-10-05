//! Source-to-native bounded mod/quotient witnesses for cml#587.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::path::PathBuf;

use cml::lisp_asm_vertical::select_arithmetic_slice;
use cml::machine_inst::{MachineInst, MachineItem, assemble_program};
use cml::native_baseline::NativeExecutable;
use cml::x86_lir::{
    LirInst, LirLowerError, LirTerminator, lower_ir_to_lir, lir_to_machine_items,
};
use cml::{lower, parser};
use sens::{ErrorKind, Session, eval_program};

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

fn oracle_session() -> Session {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("external/sens/lib/core.lisp");
    let core = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read pinned SENS core at {}: {error}", path.display()));

    let mut session = Session::default();
    eval_program(&core, &mut session)
        .expect("pinned SENS core library must load for quotient/mod oracle");
    session
}

fn oracle_u64(source: &str) -> u64 {
    let mut session = oracle_session();
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
fn bounded_divrem_lir_boundary_rejects_wrong_arity() {
    for sid in [sens::sid!(00010011), sens::sid!(00010100)] {
        for args in [vec![cml::ir::Ir::Int(7)], vec![cml::ir::Ir::Int(7), cml::ir::Ir::Int(3), cml::ir::Ir::Int(1)]] {
            let ir = cml::ir::Ir::App {
                func: Box::new(cml::ir::Ir::Sid(sid)),
                args,
            };
            let error = lower_ir_to_lir(&ir).expect_err("wrong arity must fail closed at x86 LIR boundary");
            assert!(
                matches!(
                    error,
                    LirLowerError::InvalidArity {
                        expected: 2,
                        ..
                    }
                ),
                "unexpected error for {ir:?}: {error:?}"
            );
        }
    }
}


#[test]
fn bounded_divrem_rejects_nonliteral_and_out_of_range_operands() {
    let max = cml::numeric_specialization::MAX_FIXNUM;

    for ir in [
        cml::ir::Ir::App {
            func: Box::new(cml::ir::Ir::Sid(sens::sid!(00010011))),
            args: vec![cml::ir::Ir::Var("N".to_string()), cml::ir::Ir::Int(3)],
        },
        cml::ir::Ir::App {
            func: Box::new(cml::ir::Ir::Sid(sens::sid!(00010100))),
            args: vec![cml::ir::Ir::Int(max + 1), cml::ir::Ir::Int(3)],
        },
    ] {
        let error =
            lower_ir_to_lir(&ir).expect_err("unproven operand must stay outside bounded divide");
        assert!(matches!(error, LirLowerError::Unsupported(_)));
    }
}


#[test]
fn zero_divisor_rejection_matches_upstream_named_error() {
    for source in ["(mod 5 0)", "(quotient 5 0)"] {
        let mut session = oracle_session();
        let upstream = eval_program(source, &mut session)
            .expect_err("upstream must reject zero divisor");
        assert_eq!(upstream.kind, ErrorKind::DivisionByZero);

        let ir = parse_lower_one(source);
        let local = lower_ir_to_lir(&ir)
            .expect_err("bounded x86 LIR must fail closed before hardware DIV");
        assert!(matches!(local, LirLowerError::Unsupported(_)));
    }
}


#[test]
fn mod_lir_path_matches_manual_vertical_div_mechanism_shape() {
    let source = "(mod 20 6)";
    let ir = parse_lower_one(source);

    let manual = select_arithmetic_slice(&[ir.clone()])
        .expect("current bounded manual mod oracle must still lower");
    let lir = lower_ir_to_lir(&ir).expect("bounded mod lowers to DivRem LIR");
    let plan_constraints = cml::x86_regalloc::fixed_constraints_for_function(&lir);
    let plan = cml::x86_regalloc::allocate_registers_with_constraints(&lir, &plan_constraints)
        .expect("bounded mod LIR must allocate");
    let via_lir = lir_to_machine_items(&lir).expect("bounded mod LIR must emit");

    let manual_divs = manual
        .iter()
        .filter(|item| matches!(item, MachineItem::Inst(MachineInst::DivReg { .. })))
        .count();
    let lir_divs = via_lir
        .iter()
        .filter(|item| matches!(item, MachineItem::Inst(MachineInst::DivReg { .. })))
        .count();

    assert_eq!(manual_divs, 1);
    assert_eq!(lir_divs, 1);

    for items in [&manual, &via_lir] {
        for item in items.iter() {
            if let MachineItem::Inst(MachineInst::DivReg { provenance, .. }) = item {
                assert!(
                    provenance.semantic_id.is_none(),
                    "physical divide remains mechanism-only on both paths"
                );
            }
        }
    }

    let lir_moves = via_lir
        .iter()
        .filter(|item| matches!(
            item,
            MachineItem::Inst(
                MachineInst::MovRegReg { .. }
                    | MachineInst::MovLoad { .. }
                    | MachineInst::MovStore { .. }
            )
        ))
        .count();

    eprintln!(
        "CML-DIVREM-LIR-COST source={source:?} manual_items={} lir_items={} lir_moves={} lir_spills={}",
        manual.len(),
        via_lir.len(),
        lir_moves,
        plan.spill_count
    );

    assert_eq!(
        plan.spill_count, 0,
        "small bounded literal mod witness should not need a spill"
    );
}


#[test]
fn semantic_lowering_copies_div_result_out_of_precolored_temp_immediately() {
    for (source, select_low) in [("(mod 20 6)", false), ("(quotient 20 6)", true)] {
        let ir = parse_lower_one(source);
        let lir = lower_ir_to_lir(&ir).expect("bounded source lowers");

        let (low, high) = lir.blocks[0]
            .instructions
            .iter()
            .find_map(|inst| match inst {
                LirInst::DivRem { low, high, .. } => Some((*low, *high)),
                _ => None,
            })
            .expect("DivRem must exist");

        let ret = match lir.blocks[0].terminator {
            LirTerminator::Ret { val: Some(v), .. } => v,
            ref other => panic!("bounded expression must return a value, got {other:?}"),
        };

        assert_ne!(ret, low, "semantic result must not keep RAX temp live to return");
        assert_ne!(ret, high, "semantic result must not keep RDX temp live to return");

        let intervals = cml::x86_regalloc::build_live_intervals(&lir);
        let selected = if select_low { low } else { high };
        assert!(
            intervals[&selected].end < intervals[&ret].end,
            "selected pre-colored temp must die before ordinary result vreg: source={source}, selected={:?}, ret={:?}, intervals={:?}",
            selected,
            ret,
            intervals
        );
    }
}
