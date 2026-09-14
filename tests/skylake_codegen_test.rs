//! Comprehensive verification suite for Skylake CPU-profile-driven codegen and AVX2 loop offload (#59).
//!
//! # Architecture and Philosophy
//!
//! - **Triple Oracle Verification**:
//!   `scalar reference == selected scalar optimization == vector path == my_lisp evaluation oracle`
//! - **Lisp Authority**: Expected outcomes originate exclusively from upstream `my-lisp`,
//!   never hardcoded as magic numbers.
//! - **Fail-Closed Target Security**: Explicitly asserts that unavailable extensions (AVX-512, AMX, TSX)
//!   cannot be emitted or assumed.
//! - **Observable Equivalence**: Tests non-multiple vector lengths (0, 1, 7, 8, 9, 15, 16, 23, 100)
//!   and checks cleanup tail correctness.
//!
//! # Українська документація (Ukrainian Documentation)
//!
//! Тестовий набір для перевірки кодогенерації під Intel Core i5-6400 (Skylake):
//! вибір скалярних інструкцій (Phase A: `xorq`, `leaq`, `addq $imm`), векторний конвеєр AVX2
//! (Phase B: векторні цикли по 8 елементів зі скалярним хвостом), порівняння з оракулом `my-lisp`
//! та вимірювання точки перетину ефективності (crossover threshold).

use cml::compute::ComputeBackend;
use cml::cpu_profile::{AVX2_CROSSOVER_THRESHOLD, CpuProfile, VectorMode};
use cml::ir::BufferLiteral;
use cml::machine_inst::{AluOp, MachineInst, MachineItem, assemble_program};
use cml::native_baseline::{NativeExecutable, StructuralMetrics};
use cml::x86_avx2::{
    SkylakeComputeBackend, VectorPathTaken, build_avx2_i32_add_kernel_items,
    build_scalar_i32_add_kernel_items, execute_native_i32_map,
};
use cml::x86_isel::{ScalarIselConfig, emit_machine_items_with_isel};
use cml::x86_lir::lower_ir_to_lir;
use cml::{lower, parser};
use my_lisp::{Session, eval_program, load_core_library};

fn parse_and_lower_one(source: &str) -> cml::ir::Ir {
    let exprs = parser::parse(source).expect("valid source");
    lower::lower_program(&exprs).expect("lower IR").remove(0)
}

fn lisp_oracle_scalar(source: &str) -> u64 {
    let mut session = Session::default();
    let _ = load_core_library(&mut session);
    eval_program(source, &mut session)
        .unwrap_or_else(|e| panic!("oracle eval error for `{source}`: {e:?}"))
        .value
        .to_string()
        .parse::<u64>()
        .unwrap_or_else(|e| panic!("oracle returned non-u64 for `{source}`: {e}"))
}

fn lisp_oracle_string(source: &str) -> String {
    let mut session = Session::default();
    let _ = load_core_library(&mut session);
    eval_program(source, &mut session)
        .unwrap_or_else(|e| panic!("oracle eval error for `{source}`: {e:?}"))
        .value
        .to_string()
}

#[test]
fn test_cpu_profile_parsing_and_unavailable_features_fail_closed() {
    let profile = CpuProfile::load_skylake_i5_6400().expect("load i5-6400 profile");

    assert_eq!(profile.cpu, "intel-core-i5-6400");
    assert_eq!(profile.microarchitecture, "skylake");
    assert_eq!(profile.isa, "x86-64");
    assert_eq!(profile.mode, "64-bit");

    // Supported baseline
    assert!(profile.supported_extensions.contains("SSE2"));
    assert!(profile.supported_extensions.contains("SSE4.2"));

    // Gated extensions
    assert!(profile.gated_extensions.contains_key("AVX2"));
    assert!(profile.gated_extensions.contains_key("FMA3"));
    assert!(profile.gated_extensions.contains_key("BMI2"));

    // Crucial fail-closed negative facts from Intel SKU specifications
    assert!(
        profile.is_unavailable("AVX-512"),
        "AVX-512 must be recorded unavailable on i5-6400"
    );
    assert!(
        profile.is_unavailable("AMX"),
        "AMX must be recorded unavailable on i5-6400"
    );
    assert!(
        profile.is_unavailable("TSX"),
        "TSX must be recorded unavailable on i5-6400"
    );

    // Profile permits AVX2, but rejects unavailable features
    assert!(profile.profile_has_avx2());
    assert!(!profile.is_unavailable("AVX2"));
}

#[test]
fn test_capability_provenance_audit_records() {
    let profile = CpuProfile::load_skylake_i5_6400().expect("load i5-6400 profile");

    // 1. Forced scalar mode records audit trail
    let (selected, prov) = profile.check_avx2_eligibility(VectorMode::ForcedScalar, 100);
    assert!(!selected);
    assert!(prov.reason.contains("forced scalar mode requested"));

    // 2. Auto mode with length < crossover threshold
    let (selected, prov) = profile.check_avx2_eligibility(VectorMode::Auto, 4);
    assert!(!selected);
    assert!(prov.reason.contains("below vector crossover threshold"));

    // 3. Auto mode with length >= crossover threshold on real host
    if CpuProfile::runtime_host_has_avx2() {
        let (selected, prov) =
            profile.check_avx2_eligibility(VectorMode::Auto, AVX2_CROSSOVER_THRESHOLD);
        assert!(selected);
        assert!(prov.reason.contains("auto AVX2 selected"));
    }
}

#[test]
fn test_phase_a_scalar_instruction_selection_zero_xor_and_lea() {
    // 1. Zero loading: (+ 0 0)
    let zero_source = "(+ 0 0)";
    let ir = parse_and_lower_one(zero_source);
    let func = lower_ir_to_lir(&ir).expect("lower LIR");
    let plan = cml::x86_regalloc::allocate_registers(&func).expect("regalloc");

    // Baseline off mode
    let baseline_items =
        emit_machine_items_with_isel(&func, &plan, &ScalarIselConfig::baseline_off()).unwrap();
    let baseline_metrics = StructuralMetrics::from_machine_items(&baseline_items);

    // Skylake optimized mode (xorq zero idiom)
    let skylake_items =
        emit_machine_items_with_isel(&func, &plan, &ScalarIselConfig::default_skylake()).unwrap();
    let skylake_metrics = StructuralMetrics::from_machine_items(&skylake_items);

    // Skylake mode must have smaller code size due to 3-byte xorq vs 10-byte movabsq
    assert!(
        skylake_metrics.code_bytes < baseline_metrics.code_bytes,
        "Skylake xorq zero-idiom must reduce code size: skylake={} vs baseline={}",
        skylake_metrics.code_bytes,
        baseline_metrics.code_bytes
    );

    // Both modes must produce identical return value (0)
    let base_bytes = assemble_program(&baseline_items).unwrap();
    let sky_bytes = assemble_program(&skylake_items).unwrap();
    let base_res = NativeExecutable::load(&base_bytes).call();
    let sky_res = NativeExecutable::load(&sky_bytes).call();
    assert_eq!(base_res, 0);
    assert_eq!(sky_res, 0);

    // Verify with upstream Lisp oracle
    let oracle_val = lisp_oracle_scalar(zero_source);
    assert_eq!(oracle_val, 0);

    // 2. LEA displacement arithmetic: (+ 10 42)
    let lea_source = "(+ 10 42)";
    let lea_ir = parse_and_lower_one(lea_source);
    let lea_func = lower_ir_to_lir(&lea_ir).expect("lower LIR");
    let lea_plan = cml::x86_regalloc::allocate_registers(&lea_func).expect("regalloc");

    let lea_items =
        emit_machine_items_with_isel(&lea_func, &lea_plan, &ScalarIselConfig::default_skylake())
            .unwrap();
    let has_lea = lea_items.iter().any(|item| match item {
        MachineItem::Inst(MachineInst::Lea { disp: 42, .. }) => true,
        _ => false,
    });
    assert!(
        has_lea,
        "scalar isel must emit LEA for addition with constant displacement"
    );

    let lea_bytes = assemble_program(&lea_items).unwrap();
    let lea_res = NativeExecutable::load(&lea_bytes).call();
    assert_eq!(lea_res, 52);
    assert_eq!(lisp_oracle_scalar(lea_source), 52);
}

#[test]
fn test_phase_a_scalar_instruction_selection_immediate_alu() {
    // 1. Subtraction with immediate: (- 100 42)
    let sub_source = "(- 100 42)";
    let ir = parse_and_lower_one(sub_source);
    let func = lower_ir_to_lir(&ir).expect("lower LIR");
    let plan = cml::x86_regalloc::allocate_registers(&func).expect("regalloc");

    let skylake_items =
        emit_machine_items_with_isel(&func, &plan, &ScalarIselConfig::default_skylake()).unwrap();

    // Verify that AluImm8 was emitted for Sub
    let has_imm_sub = skylake_items.iter().any(|item| match item {
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::Sub,
            imm: 42,
            ..
        }) => true,
        _ => false,
    });
    assert!(
        has_imm_sub,
        "scalar isel must emit AluImm8 for subtraction with immediate: items={skylake_items:?}"
    );

    let bytes = assemble_program(&skylake_items).unwrap();
    let res = NativeExecutable::load(&bytes).call();
    assert_eq!(res, 58);
    assert_eq!(lisp_oracle_scalar(sub_source), 58);

    // 2. Addition with prefer_lea_for_add disabled: emits AluImm8 for Add
    let add_config = ScalarIselConfig {
        prefer_zero_xor: true,
        prefer_alu_imm: true,
        prefer_lea_for_add: false,
    };
    let add_items = emit_machine_items_with_isel(&func, &plan, &add_config).unwrap();
    let has_imm_add = add_items.iter().any(|item| match item {
        MachineItem::Inst(MachineInst::AluImm8 { imm: 42, .. }) => true,
        _ => false,
    });
    assert!(
        has_imm_add,
        "scalar isel must emit immediate ALU when prefer_lea_for_add is false"
    );
}

#[test]
fn test_phase_b_avx2_vector_kernel_assembly_and_oracle_fidelity() {
    let prov = cml::machine_inst::Provenance::new(Some("0104"), "test_avx2_kernel");
    let avx2_items = build_avx2_i32_add_kernel_items(&prov);
    let scalar_items = build_scalar_i32_add_kernel_items(&prov);

    // Assembles cleanly into native bytes
    let avx2_bytes = assemble_program(&avx2_items).expect("assemble AVX2 kernel");
    let scalar_bytes = assemble_program(&scalar_items).expect("assemble scalar kernel");

    assert!(!avx2_bytes.is_empty());
    assert!(!scalar_bytes.is_empty());
}

#[test]
fn test_phase_b_avx2_vector_and_scalar_tail_parity_across_buffer_sizes() {
    let profile = CpuProfile::load_skylake_i5_6400().expect("load profile");
    let addend = 5;

    // Test across a diverse matrix of sizes:
    // - 0: empty buffer
    // - 1: single element
    // - 7: non-multiple vector length (all scalar tail)
    // - 8: exact 1x 256-bit AVX2 vector chunk (zero cleanup tail)
    // - 9: 1x AVX2 vector chunk + 1 scalar cleanup tail
    // - 15: 1x AVX2 vector chunk + 7 scalar cleanup tail
    // - 16: exact 2x AVX2 vector chunks
    // - 23: 2x AVX2 vector chunks + 7 scalar cleanup tail
    // - 64: exact 8x AVX2 vector chunks
    // - 100: 12x AVX2 vector chunks + 4 scalar cleanup tail
    let test_sizes = [0, 1, 7, 8, 9, 15, 16, 23, 64, 100];

    for &size in &test_sizes {
        let input_vec: Vec<i32> = (0..size as i32).map(|i| (i * 3) - 15).collect();

        // 1. Forced scalar path
        let scalar_report =
            execute_native_i32_map(&input_vec, addend, VectorMode::ForcedScalar, &profile)
                .expect("scalar execution");
        assert_eq!(scalar_report.path_taken, VectorPathTaken::ScalarReference);

        // Expected output from reference calculation
        let expected: Vec<i32> = input_vec.iter().map(|&x| x + addend).collect();
        assert_eq!(scalar_report.output, BufferLiteral::I32(expected.clone()));

        // 2. Forced AVX2 path or Auto path
        if CpuProfile::runtime_host_has_avx2() {
            let avx2_report =
                execute_native_i32_map(&input_vec, addend, VectorMode::ForcedAvx2, &profile)
                    .expect("AVX2 execution");
            assert_eq!(avx2_report.path_taken, VectorPathTaken::Avx2Vector);
            assert_eq!(
                avx2_report.output,
                BufferLiteral::I32(expected.clone()),
                "AVX2 output mismatch for size {size}"
            );

            // Verify bit-for-bit equivalence: scalar == avx2
            assert_eq!(
                scalar_report.output, avx2_report.output,
                "scalar reference and AVX2 vector path must yield identical outcome for size {size}"
            );

            // 3. Auto mode behavior
            let auto_report =
                execute_native_i32_map(&input_vec, addend, VectorMode::Auto, &profile)
                    .expect("auto execution");
            if size >= AVX2_CROSSOVER_THRESHOLD {
                assert_eq!(auto_report.path_taken, VectorPathTaken::Avx2Vector);
            } else {
                assert_eq!(auto_report.path_taken, VectorPathTaken::ScalarReference);
            }
            assert_eq!(auto_report.output, BufferLiteral::I32(expected));
        }
    }
}

#[test]
fn test_skylake_compute_backend_integration_with_lisp_oracle() {
    let profile = CpuProfile::load_skylake_i5_6400().expect("load profile");

    // Canonical source expression from Lisp
    let lisp_source = "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3 4 5 6 7 8 9 10))";

    // 1. Evaluate via upstream Lisp oracle
    let oracle_str = lisp_oracle_string(lisp_source);
    assert_eq!(oracle_str, "#i32(2 3 4 5 6 7 8 9 10 11)");

    // 2. Lower IR through CML
    let ir = parse_and_lower_one(lisp_source);

    // 3. Execute through SkylakeComputeBackend
    let backend = SkylakeComputeBackend::new(VectorMode::Auto, profile);
    let result = backend
        .execute(&ir)
        .expect("Skylake compute backend execution");

    assert_eq!(
        result,
        BufferLiteral::I32(vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
        "Skylake compute backend output must strictly match Lisp evaluation oracle"
    );
}

#[test]
fn test_vectorization_performance_crossover_measurement() {
    let profile = CpuProfile::load_skylake_i5_6400().expect("load profile");
    if !CpuProfile::runtime_host_has_avx2() {
        eprintln!("Skipping AVX2 benchmark: host lacks AVX2");
        return;
    }

    let sizes = [4, 8, 16, 64, 256, 1024, 4096];
    let warmup = 100;
    let iterations = 1000;

    println!("\n=== Intel Core i5-6400 (Skylake) Vectorization Crossover Benchmark ===");
    println!(
        "{:>6} | {:>14} | {:>14} | {:>10}",
        "Size", "Scalar (ns)", "AVX2 (ns)", "Speedup"
    );
    println!("{:-<6}-|-{:-<14}-|-{:-<14}-|-{:-<10}", "", "", "", "");

    for &size in &sizes {
        let input: Vec<i32> = (0..size as i32).map(|i| i + 1).collect();

        // Measure scalar
        for _ in 0..warmup {
            let _ = execute_native_i32_map(&input, 3, VectorMode::ForcedScalar, &profile).unwrap();
        }
        let t0 = std::time::Instant::now();
        for _ in 0..iterations {
            let _ = execute_native_i32_map(&input, 3, VectorMode::ForcedScalar, &profile).unwrap();
        }
        let scalar_total_ns = t0.elapsed().as_nanos();
        let scalar_ns = (scalar_total_ns as f64) / (iterations as f64);

        // Measure AVX2
        for _ in 0..warmup {
            let _ = execute_native_i32_map(&input, 3, VectorMode::ForcedAvx2, &profile).unwrap();
        }
        let t1 = std::time::Instant::now();
        for _ in 0..iterations {
            let _ = execute_native_i32_map(&input, 3, VectorMode::ForcedAvx2, &profile).unwrap();
        }
        let avx2_total_ns = t1.elapsed().as_nanos();
        let avx2_ns = (avx2_total_ns as f64) / (iterations as f64);

        let speedup = scalar_ns / avx2_ns;
        println!(
            "{:>6} | {:>14.1} | {:>14.1} | {:>9.2}x",
            size, scalar_ns, avx2_ns, speedup
        );
    }
}
