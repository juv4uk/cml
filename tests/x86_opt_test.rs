//! Conformance, differential pipeline, and negative witness tests for
//! x86 Local Optimizations in Lowered IR (#57).
//!
//! Verifies:
//! 1. Constant folding and propagation simplify constant arithmetic into single constants.
//! 2. Algebraic identities rewrite `x + 0 -> x` and `x - 0 -> x`.
//! 3. Branch simplification + unreachable block elimination fold constant conditionals
//!    and prune dead basic blocks.
//! 4. Dead Code Elimination (DCE) removes unobservable virtual-register definitions.
//! 5. Differential matrix:
//!    - opt off -> native actual
//!    - individual pass on -> native actual
//!    - full local pipeline -> native actual
//!    All match the upstream Lisp oracle with fewer instructions and code bytes.
//! 6. Negative witnesses:
//!    - overflow-sensitive arithmetic does NOT illegally fold;
//!    - dynamic/unknown values block folding and remain real machine instructions.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::machine_inst::assemble_program;
use cml::native_baseline::{NativeExecutable, StructuralMetrics};
use cml::numeric_specialization::MAX_FIXNUM;
use cml::x86_lir::{
    LirAluOp, LirCond, LirFunction, LirInst, LirTerminator, VReg, lir_to_machine_items,
    lower_ir_to_lir,
};
use cml::x86_opt::{LocalOptConfig, optimize_lir};
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

fn lisp_oracle(source: &str) -> u64 {
    let mut session = Session::default();
    eval_program(source, &mut session)
        .unwrap_or_else(|e| panic!("oracle eval error for `{source}`: {e:?}"))
        .value
        .to_string()
        .parse::<u64>()
        .unwrap_or_else(|e| panic!("oracle returned non-u64 for `{source}`: {e}"))
}

#[test]
fn test_constant_folding_and_propagation() {
    let source = "(+ (+ 10 20) 12)";
    let ir = parse_and_lower_ir(source);
    let mut func = lower_ir_to_lir(&ir).expect("lower to LIR");

    let initial_inst_count: usize = func.blocks.iter().map(|b| b.instructions.len()).sum();

    // Enable constant folding and propagation
    let mut config = LocalOptConfig::all_disabled();
    config.const_folding = true;
    config.const_propagation = true;
    config.dead_code_elimination = true;

    let report = optimize_lir(&mut func, config);

    assert!(
        report.consts_folded >= 2,
        "both additions must be folded: got {}",
        report.consts_folded
    );

    let opt_inst_count: usize = func.blocks.iter().map(|b| b.instructions.len()).sum();
    assert!(
        opt_inst_count < initial_inst_count,
        "optimized instruction count ({opt_inst_count}) must be less than unoptimized ({initial_inst_count})"
    );

    // Native execution
    let items = lir_to_machine_items(&func).expect("emit machine items");
    let bytes = assemble_program(&items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    let result = exec.call();

    let oracle_val = lisp_oracle(source);
    assert_eq!(result, 42);
    assert_eq!(result, oracle_val);
}

#[test]
fn test_algebraic_identities() {
    // Construct LIR for: v0 = 42, v1 = 0, v2 = v0 + v1, ret v2
    let prov = cml::machine_inst::Provenance::new(Some("0104"), "test_algebraic");
    let mut func = LirFunction::new("algebraic_fn", prov.clone());
    let entry = func.entry;
    let block = func.block_mut(entry).unwrap();

    let v0 = VReg(0);
    let v1 = VReg(1);
    let v2 = VReg(2);

    block.instructions.push(LirInst::Const64 {
        dst: v0,
        imm: 42,
        provenance: prov.clone(),
    });
    block.instructions.push(LirInst::Const64 {
        dst: v1,
        imm: 0,
        provenance: prov.clone(),
    });
    block.instructions.push(LirInst::Alu {
        op: LirAluOp::Add,
        dst: v2,
        lhs: v0,
        rhs: v1,
        provenance: prov.clone(),
    });
    block.terminator = LirTerminator::Ret {
        val: Some(v2),
        provenance: prov,
    };

    let mut config = LocalOptConfig::all_disabled();
    config.algebraic_identities = true;
    config.copy_propagation = true;
    config.dead_code_elimination = true;

    let report = optimize_lir(&mut func, config);
    assert!(
        report.identities_applied >= 1,
        "x + 0 identity must be applied"
    );

    let items = lir_to_machine_items(&func).expect("emit machine items");
    let bytes = assemble_program(&items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    assert_eq!(exec.call(), 42);
}

#[test]
fn test_branch_simplification_and_unreachable_block_elimination() {
    let source = "(cond ((eq 5 5) 42) (t 99))";
    let ir = parse_and_lower_ir(source);
    let mut func = lower_ir_to_lir(&ir).expect("lower to LIR");

    let initial_block_count = func.blocks.len();
    assert!(initial_block_count >= 3);

    let mut config = LocalOptConfig::all_disabled();
    config.const_folding = true;
    config.branch_simplification = true;
    config.unreachable_block_elimination = true;

    let report = optimize_lir(&mut func, config);

    assert_eq!(
        report.branches_simplified, 1,
        "constant 5 == 5 condition must be simplified to unconditional jump"
    );
    assert!(
        report.blocks_eliminated >= 1,
        "unreachable false branch block must be pruned"
    );
    assert!(func.blocks.len() < initial_block_count);

    let items = lir_to_machine_items(&func).expect("emit machine items");
    let bytes = assemble_program(&items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    let result = exec.call();

    let oracle_val = lisp_oracle(source);
    assert_eq!(result, 42);
    assert_eq!(result, oracle_val);
}

#[test]
fn test_dead_code_elimination() {
    // Construct LIR with an unused constant and an unused calculation
    let prov = cml::machine_inst::Provenance::new(Some("0104"), "test_dce");
    let mut func = LirFunction::new("dce_fn", prov.clone());
    let entry = func.entry;
    let block = func.block_mut(entry).unwrap();

    let v_dead1 = VReg(0);
    let v_dead2 = VReg(1);
    let v_live = VReg(2);

    block.instructions.push(LirInst::Const64 {
        dst: v_dead1,
        imm: 999,
        provenance: prov.clone(),
    });
    block.instructions.push(LirInst::Const64 {
        dst: v_live,
        imm: 42,
        provenance: prov.clone(),
    });
    block.instructions.push(LirInst::Alu {
        op: LirAluOp::Add,
        dst: v_dead2,
        lhs: v_dead1,
        rhs: v_live,
        provenance: prov.clone(),
    });
    block.terminator = LirTerminator::Ret {
        val: Some(v_live),
        provenance: prov,
    };

    let mut config = LocalOptConfig::all_disabled();
    config.dead_code_elimination = true;

    let report = optimize_lir(&mut func, config);
    assert!(
        report.dce_removed >= 2,
        "both dead instructions must be removed by DCE"
    );

    let items = lir_to_machine_items(&func).expect("emit machine items");
    let bytes = assemble_program(&items).expect("assemble machine code");
    let exec = NativeExecutable::load(&bytes);
    assert_eq!(exec.call(), 42);
}

#[test]
fn test_differential_pipeline_matrix() {
    let source = "(+ (+ 10 20) 12)";
    let ir = parse_and_lower_ir(source);
    let oracle_val = lisp_oracle(source);

    // 1. Opt OFF
    let func_unopt = lower_ir_to_lir(&ir).expect("lower unopt");
    let items_unopt = lir_to_machine_items(&func_unopt).expect("emit unopt");
    let bytes_unopt = assemble_program(&items_unopt).expect("assemble unopt");
    let res_unopt = NativeExecutable::load(&bytes_unopt).call();
    let metrics_unopt = StructuralMetrics::from_machine_items(&items_unopt);

    // 2. Individual pass ON: constant folding only
    let mut func_fold = lower_ir_to_lir(&ir).expect("lower fold");
    let mut cfg_fold = LocalOptConfig::all_disabled();
    cfg_fold.const_folding = true;
    optimize_lir(&mut func_fold, cfg_fold);
    let items_fold = lir_to_machine_items(&func_fold).expect("emit fold");
    let bytes_fold = assemble_program(&items_fold).expect("assemble fold");
    let res_fold = NativeExecutable::load(&bytes_fold).call();

    // 3. Full pipeline ON (all_enabled)
    let mut func_opt = lower_ir_to_lir(&ir).expect("lower opt");
    let report = optimize_lir(&mut func_opt, LocalOptConfig::all_enabled());
    assert!(report.total_transforms() > 0);
    let items_opt = lir_to_machine_items(&func_opt).expect("emit opt");
    let bytes_opt = assemble_program(&items_opt).expect("assemble opt");
    let res_opt = NativeExecutable::load(&bytes_opt).call();
    let metrics_opt = StructuralMetrics::from_machine_items(&items_opt);

    // Semantic parity invariant across all passes
    assert_eq!(res_unopt, oracle_val);
    assert_eq!(res_fold, oracle_val);
    assert_eq!(res_opt, oracle_val);

    // Structural evidence: optimized code has fewer instructions and smaller byte size
    assert!(
        metrics_opt.instruction_count < metrics_unopt.instruction_count,
        "full pipeline must reduce instruction count: {} vs {}",
        metrics_opt.instruction_count,
        metrics_unopt.instruction_count
    );
    assert!(
        metrics_opt.code_bytes < metrics_unopt.code_bytes,
        "full pipeline must reduce code bytes: {} vs {}",
        metrics_opt.code_bytes,
        metrics_unopt.code_bytes
    );
}

#[test]
fn test_negative_witnesses() {
    // 1. Overflow-sensitive: MAX_FIXNUM + 10 must NOT fold
    let prov = cml::machine_inst::Provenance::new(Some("0104"), "test_overflow");
    let mut func = LirFunction::new("overflow_fn", prov.clone());
    let entry = func.entry;
    let block = func.block_mut(entry).unwrap();

    let v0 = VReg(0);
    let v1 = VReg(1);
    let v2 = VReg(2);

    block.instructions.push(LirInst::Const64 {
        dst: v0,
        imm: MAX_FIXNUM as u64,
        provenance: prov.clone(),
    });
    block.instructions.push(LirInst::Const64 {
        dst: v1,
        imm: 10,
        provenance: prov.clone(),
    });
    block.instructions.push(LirInst::Alu {
        op: LirAluOp::Add,
        dst: v2,
        lhs: v0,
        rhs: v1,
        provenance: prov.clone(),
    });
    block.terminator = LirTerminator::Ret {
        val: Some(v2),
        provenance: prov.clone(),
    };

    let report = optimize_lir(&mut func, LocalOptConfig::all_enabled());
    assert_eq!(
        report.consts_folded, 0,
        "overflowing operation must NOT be illegally folded"
    );

    // 2. Dynamic condition: non-constant Cmp must NOT simplify branch
    let mut func_dyn = LirFunction::new("dynamic_branch_fn", prov.clone());
    let entry_d = func_dyn.entry;
    let b1 = func_dyn.create_block();
    let b2 = func_dyn.create_block();

    let v_a = VReg(0);
    let v_b = VReg(1);

    // v_a and v_b are uninitialized / dynamic registers (not constants)
    let block_d = func_dyn.block_mut(entry_d).unwrap();
    block_d.instructions.push(LirInst::Cmp {
        lhs: v_a,
        rhs: v_b,
        provenance: prov.clone(),
    });
    block_d.terminator = LirTerminator::BranchCond {
        cond: LirCond::Equal,
        true_block: b1,
        false_block: b2,
        provenance: prov.clone(),
    };

    let report_dyn = optimize_lir(&mut func_dyn, LocalOptConfig::all_enabled());
    assert_eq!(
        report_dyn.branches_simplified, 0,
        "dynamic comparison must remain a real conditional branch"
    );
    assert_eq!(
        report_dyn.blocks_eliminated, 0,
        "neither branch block may be eliminated when condition is dynamic"
    );
}
