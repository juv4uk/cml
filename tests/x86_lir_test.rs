//! Conformance, CFG, and semantic parity tests for x86 Lowered IR (LIR) (#54).
//!
//! Verifies:
//! 1. Lowering from backend-neutral `Ir` to `LirFunction` (CFG + virtual registers).
//! 2. Arithmetic witness (`(+ 10 32) -> 42`) executes natively with semantic parity.
//! 3. Branch/`cond` witness executes through real basic blocks with conditional jumps.
//! 4. Parity with the upstream `my-lisp` evaluation oracle.
//! 5. Determinism of generated machine sequences.
//! 6. Inspectable dump projection of blocks and vregs.
//! 7. Fail-closed rejection of unadmitted forms.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::machine_inst::assemble_program;
use cml::native_baseline::NativeExecutable;
use cml::x86_lir::{LirLowerError, lir_to_machine_items, lower_ir_to_lir};
use cml::{lower, parser};
use my_lisp::{Session, eval_program};

fn parse_and_lower_ir(source: &str) -> cml::ir::Ir {
    let exprs =
        parser::parse(source).unwrap_or_else(|e| panic!("parse error for `{source}`: {e:?}"));
    let mut lowered = lower::lower_program(&exprs)
        .unwrap_or_else(|e| panic!("lowering error for `{source}`: {e}"));
    assert_eq!(lowered.len(), 1);
    lowered.remove(0)
}

fn lisp_oracle(source: &str) -> String {
    let mut session = Session::default();
    eval_program(source, &mut session)
        .unwrap_or_else(|e| panic!("oracle eval error for `{source}`: {e:?}"))
        .value
        .to_string()
}

#[test]
fn test_lir_arithmetic_witness_native_parity() {
    let source = "(+ 10 32)";
    let ir = parse_and_lower_ir(source);

    // 1. Lower to x86 LIR (CFG + VRegs)
    let lir_func = lower_ir_to_lir(&ir).expect("lower to LIR");
    assert_eq!(
        lir_func.blocks.len(),
        1,
        "straight-line arithmetic has 1 basic block"
    );

    let dump = lir_func.dump();
    assert!(dump.contains("function main"));
    assert!(dump.contains("add"));
    assert!(dump.contains("ret"));

    // 2. Emit to structured MachineItems
    let machine_items = lir_to_machine_items(&lir_func).expect("emit to machine items");

    // 3. Assemble and execute natively
    let bytes = assemble_program(&machine_items).expect("assemble machine items");
    let exec = NativeExecutable::load(&bytes);
    let native_result = exec.call();

    // 4. Verify against Lisp oracle
    let oracle_val: u64 = lisp_oracle(source).parse().unwrap();
    assert_eq!(native_result, 42);
    assert_eq!(native_result, oracle_val);
}

#[test]
fn test_lir_cond_branch_true_path_witness() {
    let source = "(cond ((eq 5 5) 42) (t 99))";
    let ir = parse_and_lower_ir(source);

    let lir_func = lower_ir_to_lir(&ir).expect("lower cond to LIR");
    assert!(
        lir_func.blocks.len() >= 3,
        "cond with two branches must produce at least 3 basic blocks (entry, then, join/else)"
    );

    let dump = lir_func.dump();
    assert!(dump.contains("cmp"));
    assert!(dump.contains("branch_eq"));
    assert!(dump.contains("jmp"));

    let machine_items = lir_to_machine_items(&lir_func).expect("emit to machine items");
    let bytes = assemble_program(&machine_items).expect("assemble cond");
    let exec = NativeExecutable::load(&bytes);
    let native_result = exec.call();

    let oracle_val: u64 = lisp_oracle(source).parse().unwrap();
    assert_eq!(native_result, 42, "true branch must be taken (5 == 5)");
    assert_eq!(native_result, oracle_val);
}

#[test]
fn test_lir_cond_branch_false_path_witness() {
    let source = "(cond ((eq 5 6) 42) (t 99))";
    let ir = parse_and_lower_ir(source);

    let lir_func = lower_ir_to_lir(&ir).expect("lower cond to LIR");
    let machine_items = lir_to_machine_items(&lir_func).expect("emit to machine items");
    let bytes = assemble_program(&machine_items).expect("assemble cond");
    let exec = NativeExecutable::load(&bytes);
    let native_result = exec.call();

    let oracle_val: u64 = lisp_oracle(source).parse().unwrap();
    assert_eq!(native_result, 99, "else branch must be taken (5 != 6)");
    assert_eq!(native_result, oracle_val);
}

#[test]
fn test_lir_subtraction_expression_witness() {
    let source = "(- 100 58)";
    let ir = parse_and_lower_ir(source);

    let lir_func = lower_ir_to_lir(&ir).expect("lower sub to LIR");
    let machine_items = lir_to_machine_items(&lir_func).expect("emit to machine items");
    let bytes = assemble_program(&machine_items).expect("assemble sub");
    let exec = NativeExecutable::load(&bytes);
    let native_result = exec.call();

    let oracle_val: u64 = lisp_oracle(source).parse().unwrap();
    assert_eq!(native_result, 42);
    assert_eq!(native_result, oracle_val);
}

#[test]
fn test_lir_emission_determinism() {
    let source = "(cond ((eq 10 10) (+ 20 22)) (t (- 100 1)))";
    let ir1 = parse_and_lower_ir(source);
    let ir2 = parse_and_lower_ir(source);

    let lir1 = lower_ir_to_lir(&ir1).unwrap();
    let lir2 = lower_ir_to_lir(&ir2).unwrap();

    assert_eq!(lir1.dump(), lir2.dump(), "LIR dump must be deterministic");

    let items1 = lir_to_machine_items(&lir1).unwrap();
    let items2 = lir_to_machine_items(&lir2).unwrap();

    let bytes1 = assemble_program(&items1).unwrap();
    let bytes2 = assemble_program(&items2).unwrap();

    assert_eq!(
        bytes1, bytes2,
        "machine bytes must be byte-for-byte deterministic across runs"
    );
}

#[test]
fn test_lir_cfg_predecessors_and_successors() {
    let source = "(cond ((eq 1 1) 10) (t 20))";
    let ir = parse_and_lower_ir(source);
    let func = lower_ir_to_lir(&ir).unwrap();

    let entry = func.entry;
    let entry_succs = func.block(entry).unwrap().terminator.successors();
    assert_eq!(
        entry_succs.len(),
        2,
        "entry block branches conditionally into two successors"
    );

    for succ in entry_succs {
        let preds = func.predecessors(succ);
        assert!(
            preds.contains(&entry),
            "successor block must have entry as predecessor"
        );
    }
}

#[test]
fn test_unadmitted_forms_fail_closed() {
    let bad_source = "(quote (a b c))";
    let ir = parse_and_lower_ir(bad_source);
    assert!(matches!(
        lower_ir_to_lir(&ir),
        Err(LirLowerError::Unsupported(_))
    ));
}
