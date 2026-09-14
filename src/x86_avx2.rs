//! Skylake AVX2 vector codegen and loop offload (#59 Phase B).
//!
//! # Architecture and Philosophy
//!
//! - **Target CPU Profile Gate**: AVX2 instructions are generated and executed only if the
//!   target CPU profile permits them and runtime host CPUID/OSXSAVE/XGETBV verification passes.
//! - **Compute IR Synergy**: Works on pure element-wise contiguous integer buffer operations
//!   proven by `cml::compute::ComputeAnalysis`.
//! - **Vector + Scalar Cleanup Tail**:
//!   - 256-bit AVX2 vectors process 8 x 32-bit integers per iteration (`vmovdqu`, `vpaddd`, `vmovdqu`).
//!   - Scalar cleanup loop correctly handles remaining elements (0..7) without padding or buffer overruns.
//!   - `vzeroupper` restores upper YMM state to avoid AVX-SSE transition penalties on Skylake.
//! - **Oracle & Reference Parity**:
//!   `scalar reference == selected scalar optimization == vector path`
//!   verified against upstream `my-lisp` evaluation oracle.
//!
//! # Українська документація (Ukrainian Documentation)
//!
//! Цей модуль реалізує генерацію та виконання векторного коду AVX2 для процесора Intel Core i5-6400.
//! Він підтримує векторні цикли по 8 елементів (`i32`), скалярний хвіст (cleanup tail) для залишків,
//! інструкцію `vzeroupper` та перевірку походження можливостей (capability provenance).

use crate::compute::{
    AdmissionBlocker, BulkOperation, ComputeBackend, ComputeExecutionError, EffectClass,
    ExecutionShape, NumericDomain, StorageClass, analyze,
};
use crate::cpu_profile::{CapabilityProvenance, CpuProfile, VectorMode};
use crate::ir::{BufferLiteral, Ir};
use crate::machine_inst::{
    AluOp, CondCode, MachineInst, MachineItem, Provenance, X86Reg, XmmReg, YmmReg, assemble_program,
};
use crate::native_baseline::{NativeExecutable, StructuralMetrics};

/// Description of execution path taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorPathTaken {
    Avx2Vector,
    ScalarReference,
}

/// Detailed diagnostic report of Skylake buffer execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkylakeExecutionReport {
    pub output: BufferLiteral,
    pub mode_requested: VectorMode,
    pub path_taken: VectorPathTaken,
    pub provenance: CapabilityProvenance,
    pub structural: StructuralMetrics,
}

/// Builds machine items for an AVX2 256-bit element-wise integer add kernel with scalar cleanup.
pub fn build_avx2_i32_add_kernel_items(prov: &Provenance) -> Vec<MachineItem> {
    vec![
        // Check if length is 0: test %rdx, %rdx; jz .Ldone
        MachineItem::Inst(MachineInst::TestRegReg {
            reg1: X86Reg::Rdx,
            reg2: X86Reg::Rdx,
            provenance: prov.clone(),
        }),
        MachineItem::JccLabel {
            cond: CondCode::Equal,
            target: ".Ldone".to_string(),
            provenance: prov.clone(),
        },
        // Broadcast addend in %ecx to %ymm1: vmovd %ecx, %xmm1; vpbroadcastd %xmm1, %ymm1
        MachineItem::Inst(MachineInst::VmovdGprToXmm {
            dst: XmmReg::Xmm1,
            src: X86Reg::Rcx,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Vpbroadcastd {
            dst: YmmReg::Ymm1,
            src: XmmReg::Xmm1,
            provenance: prov.clone(),
        }),
        // Calculate number of 8-element chunks: mov %rdx, %rax; shr $3, %rax; test %rax, %rax; jz .Lscalar_loop
        MachineItem::Inst(MachineInst::MovRegReg {
            dst: X86Reg::Rax,
            src: X86Reg::Rdx,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::ShrImm {
            reg: X86Reg::Rax,
            imm: 3,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::TestRegReg {
            reg1: X86Reg::Rax,
            reg2: X86Reg::Rax,
            provenance: prov.clone(),
        }),
        MachineItem::JccLabel {
            cond: CondCode::Equal,
            target: ".Lscalar_loop".to_string(),
            provenance: prov.clone(),
        },
        // .Lvector_loop:
        MachineItem::Label(".Lvector_loop".to_string()),
        MachineItem::Inst(MachineInst::VmovdquLoad {
            dst: YmmReg::Ymm0,
            base: X86Reg::Rdi,
            disp: 0,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Vpaddd {
            dst: YmmReg::Ymm0,
            src1: YmmReg::Ymm0,
            src2: YmmReg::Ymm1,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::VmovdquStore {
            base: X86Reg::Rsi,
            disp: 0,
            src: YmmReg::Ymm0,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::Add,
            dst: X86Reg::Rdi,
            imm: 32,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::Add,
            dst: X86Reg::Rsi,
            imm: 32,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::Sub,
            dst: X86Reg::Rax,
            imm: 1,
            provenance: prov.clone(),
        }),
        MachineItem::JccLabel {
            cond: CondCode::NotEqual,
            target: ".Lvector_loop".to_string(),
            provenance: prov.clone(),
        },
        // .Lscalar_loop:
        MachineItem::Label(".Lscalar_loop".to_string()),
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::And,
            dst: X86Reg::Rdx,
            imm: 7,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::TestRegReg {
            reg1: X86Reg::Rdx,
            reg2: X86Reg::Rdx,
            provenance: prov.clone(),
        }),
        MachineItem::JccLabel {
            cond: CondCode::Equal,
            target: ".Lvector_done".to_string(),
            provenance: prov.clone(),
        },
        // .Lscalar_tail:
        MachineItem::Label(".Lscalar_tail".to_string()),
        MachineItem::Inst(MachineInst::MovLoad32 {
            dst: X86Reg::Rax,
            base: X86Reg::Rdi,
            disp: 0,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Alu32RegReg {
            op: AluOp::Add,
            dst: X86Reg::Rax,
            src: X86Reg::Rcx,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::MovStore32 {
            base: X86Reg::Rsi,
            disp: 0,
            src: X86Reg::Rax,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::Add,
            dst: X86Reg::Rdi,
            imm: 4,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::Add,
            dst: X86Reg::Rsi,
            imm: 4,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::Sub,
            dst: X86Reg::Rdx,
            imm: 1,
            provenance: prov.clone(),
        }),
        MachineItem::JccLabel {
            cond: CondCode::NotEqual,
            target: ".Lscalar_tail".to_string(),
            provenance: prov.clone(),
        },
        // .Lvector_done:
        MachineItem::Label(".Lvector_done".to_string()),
        MachineItem::Inst(MachineInst::Vzeroupper {
            provenance: prov.clone(),
        }),
        // .Ldone:
        MachineItem::Label(".Ldone".to_string()),
        MachineItem::Inst(MachineInst::Ret {
            provenance: prov.clone(),
        }),
    ]
}

/// Builds machine items for the reference scalar integer add loop.
pub fn build_scalar_i32_add_kernel_items(prov: &Provenance) -> Vec<MachineItem> {
    vec![
        MachineItem::Inst(MachineInst::TestRegReg {
            reg1: X86Reg::Rdx,
            reg2: X86Reg::Rdx,
            provenance: prov.clone(),
        }),
        MachineItem::JccLabel {
            cond: CondCode::Equal,
            target: ".Lscalar_done".to_string(),
            provenance: prov.clone(),
        },
        MachineItem::Label(".Lscalar_loop".to_string()),
        MachineItem::Inst(MachineInst::MovLoad32 {
            dst: X86Reg::Rax,
            base: X86Reg::Rdi,
            disp: 0,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::Alu32RegReg {
            op: AluOp::Add,
            dst: X86Reg::Rax,
            src: X86Reg::Rcx,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::MovStore32 {
            base: X86Reg::Rsi,
            disp: 0,
            src: X86Reg::Rax,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::Add,
            dst: X86Reg::Rdi,
            imm: 4,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::Add,
            dst: X86Reg::Rsi,
            imm: 4,
            provenance: prov.clone(),
        }),
        MachineItem::Inst(MachineInst::AluImm8 {
            op: AluOp::Sub,
            dst: X86Reg::Rdx,
            imm: 1,
            provenance: prov.clone(),
        }),
        MachineItem::JccLabel {
            cond: CondCode::NotEqual,
            target: ".Lscalar_loop".to_string(),
            provenance: prov.clone(),
        },
        MachineItem::Label(".Lscalar_done".to_string()),
        MachineItem::Inst(MachineInst::Ret {
            provenance: prov.clone(),
        }),
    ]
}

/// Executes a native integer buffer map directly on the physical host via SysV ABI call.
pub fn execute_native_i32_map(
    input: &[i32],
    addend: i32,
    mode: VectorMode,
    profile: &CpuProfile,
) -> Result<SkylakeExecutionReport, String> {
    let (use_avx2, provenance) = profile.check_avx2_eligibility(mode, input.len());

    if mode == VectorMode::ForcedAvx2 && !use_avx2 {
        return Err(format!("forced AVX2 failed: {}", provenance.reason));
    }

    let prov = Provenance::new(Some("0104"), "execute_native_i32_map");
    let (items, path_taken) = if use_avx2 {
        (
            build_avx2_i32_add_kernel_items(&prov),
            VectorPathTaken::Avx2Vector,
        )
    } else {
        (
            build_scalar_i32_add_kernel_items(&prov),
            VectorPathTaken::ScalarReference,
        )
    };

    let structural = StructuralMetrics::from_machine_items(&items);
    let code_bytes = assemble_program(&items)
        .map_err(|e| format!("failed to assemble native buffer kernel: {e}"))?;

    let mut output = vec![0i32; input.len()];

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        if !input.is_empty() {
            let exe = NativeExecutable::load(&code_bytes);
            exe.call_i32_kernel(input.as_ptr(), output.as_mut_ptr(), input.len(), addend);
        }
    }

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        for (idx, val) in input.iter().enumerate() {
            output[idx] = val.wrapping_add(addend);
        }
    }

    Ok(SkylakeExecutionReport {
        output: BufferLiteral::I32(output),
        mode_requested: mode,
        path_taken,
        provenance,
        structural,
    })
}

/// ComputeBackend implementation targeting Skylake native capabilities.
pub struct SkylakeComputeBackend {
    pub mode: VectorMode,
    pub profile: CpuProfile,
}

impl SkylakeComputeBackend {
    pub fn new(mode: VectorMode, profile: CpuProfile) -> Self {
        Self { mode, profile }
    }

    pub fn execute_diagnostic(
        &self,
        ir: &Ir,
    ) -> Result<SkylakeExecutionReport, ComputeExecutionError> {
        let analysis = analyze(ir);
        if !analysis.gpu_eligible() {
            return Err(ComputeExecutionError::NotEligible(analysis.gpu_blockers));
        }

        let Some(region) = &analysis.region else {
            return Err(ComputeExecutionError::UnsupportedOperation);
        };

        if region.operation != BulkOperation::Map {
            return Err(ComputeExecutionError::UnsupportedOperation);
        }

        let Some(kernel) = &region.kernel else {
            return Err(ComputeExecutionError::UnsupportedOperation);
        };

        if analysis.shape != ExecutionShape::ElementWise
            || analysis.effect != EffectClass::Pure
            || analysis.storage != StorageClass::ContiguousBuffer
            || analysis.numeric_domain != NumericDomain::FixedWidthInteger
        {
            return Err(ComputeExecutionError::NotEligible(vec![
                AdmissionBlocker::NotBulkParallel,
            ]));
        }

        let Ir::Buffer(BufferLiteral::I32(ref input)) = region.input else {
            return Err(ComputeExecutionError::UnsupportedOperation);
        };

        // Extract affine addend offset: parameter-0 + C
        let addend = crate::compute::f32_affine_offset(&kernel.body)
            .ok_or(ComputeExecutionError::InternalInvariant)? as i32;

        execute_native_i32_map(input, addend, self.mode, &self.profile)
            .map_err(|_| ComputeExecutionError::InternalInvariant)
    }
}

impl ComputeBackend for SkylakeComputeBackend {
    fn execute(&self, ir: &Ir) -> Result<BufferLiteral, ComputeExecutionError> {
        self.execute_diagnostic(ir).map(|report| report.output)
    }
}
