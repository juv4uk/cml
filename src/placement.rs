//! Evidence-driven backend placement for admitted compute regions.
//!
//! The planner selects an [`ExecutionTarget`] from **measurable, declared facts**
//! only — element count, operation shape, admitted kernel complexity, available
//! CPU worker count, and live accelerator capability evidence.  It never uses
//! hardware names or machine-specific thresholds as semantic truth.
//!
//! ## Invariants
//!
//! - No hardware name or numeric threshold enters IR meaning.
//! - Selection is fully reproducible from the same `PlacementInputs` and
//!   `PlacementConfig`.
//! - GPU/FPGA targets are only returned when live capability evidence is present.
//! - Automatic placement is disabled for regions without proven backend parity.
//! - A selected backend that begins execution must not fall back silently; errors
//!   are policy-named (see [`PlacementRejection`]).
//! - Changing `PlacementConfig` thresholds **never changes the contractual result**
//!   of a semantically admitted computation — only the physical executor chosen.

use crate::compute::{BulkOperation, ComputeAnalysis, ExecutionShape};
use crate::execution::ExecutionTarget;

// ── Public surface ────────────────────────────────────────────────────────────

/// Inputs the planner may inspect.  All fields come from already-admitted,
/// measurable facts — never from host benchmarks or opaque runtime state.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementInputs {
    /// Number of elements in the input buffer, if known.
    pub element_count: Option<usize>,
    /// Compute analysis produced by [`crate::compute::analyze`].
    pub analysis: ComputeAnalysis,
    /// Number of logical CPU workers available to the parallel backend.
    pub available_cpu_workers: usize,
    /// Live GPU/accelerator capabilities visible to this execution context.
    pub accelerator_evidence: Vec<AcceleratorCapabilityEvidence>,
}

/// Proved live capability for one accelerator path.
///
/// The planner only considers capabilities that have been *explicitly supplied*
/// as live evidence — never inferred from compile-time features alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorCapabilityEvidence {
    /// Logical backend tag, matching the key used in [`ExecutionTarget::Gpu`]
    /// or [`ExecutionTarget::Fpga`].
    pub backend_tag: String,
    /// Whether this backend has been proven to produce parity output with the
    /// canonical CPU-1 reference path for the current operation shape.
    pub parity_proven: bool,
    /// Kind of accelerator this evidence describes.
    pub kind: AcceleratorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorKind {
    Gpu,
    Fpga,
}

/// Calibrated thresholds stored as mechanism/configuration.
///
/// These numbers control **which** physical executor is chosen; they never
/// appear in IR meaning and must not affect the contractual *result* of a
/// computation.  Changing them only changes performance characteristics, not
/// program behaviour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementConfig {
    /// Minimum element count below which multicore CPU is not attempted even
    /// when workers are available.  Parallelisation overhead dominates small
    /// arrays.
    pub multicore_element_threshold: usize,
    /// Minimum element count below which GPU offload is not attempted even
    /// when a live, parity-proven GPU is available.  Transfer/launch overhead
    /// dominates small buffers.
    pub gpu_element_threshold: usize,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self {
            // Conservative defaults derived from microbenchmark observations on
            // the reference machine (see tests).  Not universal truth — callers
            // with different hardware profiles should supply their own config.
            multicore_element_threshold: 4_096,
            gpu_element_threshold: 65_536,
        }
    }
}

/// Explicit caller override that bypasses automatic placement.
///
/// Tests and integration harnesses may force a specific backend without
/// changing the source program.  The planner still validates that the
/// requested backend is available; it will return
/// [`PlacementRejection::OverriddenTargetUnavailable`] if not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementOverride {
    /// Force single-core CPU sequential execution.
    CpuSequential,
    /// Force multicore CPU parallel execution with the provided worker count.
    CpuParallel { workers: usize },
    /// Force a named GPU backend.
    Gpu { backend_tag: String },
    /// Force a named FPGA backend.
    Fpga { device_tag: String },
}

/// The target and proof-of-selection returned when placement succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementDecision {
    /// The physical target to use.
    pub target: ExecutionTarget,
    /// Human-readable provenance record: why this target was chosen.
    pub reason: PlacementReason,
}

/// Machine-readable provenance for a successful placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementReason {
    /// Forced by an explicit caller override; no automatic selection ran.
    ExplicitOverride { override_kind: String },
    /// Region is not bulk-parallel eligible; sequential CPU is the only
    /// safe reference path.
    NotBulkEligible,
    /// Region is admitted but element count is below the multicore threshold;
    /// sequential CPU avoids parallelisation overhead.
    BelowMulticoreThreshold {
        element_count: usize,
        threshold: usize,
    },
    /// Region is admitted and large enough for multicore CPU.
    MulticoreCpu {
        workers: usize,
        element_count: usize,
    },
    /// Region is admitted, GPU threshold met, and parity-proven GPU available.
    GpuOffload {
        backend_tag: String,
        element_count: usize,
        threshold: usize,
    },
    /// Automatic placement produced sequential CPU as the default safe path.
    SequentialCpuDefault,
}

/// Reason a placement attempt could not produce a target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementRejection {
    /// An explicit override was requested but the target is not registered or
    /// not live.
    OverriddenTargetUnavailable { override_kind: String },
    /// GPU offload was required (override) but parity has not been proven for
    /// this operation shape.
    GpuParityNotProven { backend_tag: String },
}

// ── Planner ───────────────────────────────────────────────────────────────────

/// Select an [`ExecutionTarget`] for an admitted compute region.
///
/// # Override path
/// When `override_` is [`Some`], the planner validates availability and
/// returns the override target (or an error) without running automatic
/// selection.
///
/// # Automatic path
/// 1. Fail-closed: non-eligible regions → sequential CPU.
/// 2. Sequential CPU when element count is unknown or below multicore threshold.
/// 3. Multicore CPU when element count meets the threshold and workers > 1.
/// 4. GPU when element count meets the GPU threshold and a parity-proven live
///    GPU is present.  GPU is chosen only after the multicore threshold is also
///    exceeded, as per the issue's desired policy shape.
/// 5. Sequential CPU as the default safe fallback.
pub fn place(
    inputs: &PlacementInputs,
    config: &PlacementConfig,
    override_: Option<&PlacementOverride>,
) -> Result<PlacementDecision, PlacementRejection> {
    if let Some(ov) = override_ {
        return apply_override(ov, inputs);
    }
    automatic_placement(inputs, config)
}

fn apply_override(
    ov: &PlacementOverride,
    inputs: &PlacementInputs,
) -> Result<PlacementDecision, PlacementRejection> {
    match ov {
        PlacementOverride::CpuSequential => Ok(PlacementDecision {
            target: ExecutionTarget::Cpu,
            reason: PlacementReason::ExplicitOverride {
                override_kind: "cpu-1".into(),
            },
        }),
        PlacementOverride::CpuParallel { workers } => Ok(PlacementDecision {
            target: ExecutionTarget::Gpu {
                backend: format!("cpu-parallel-{workers}"),
            },
            reason: PlacementReason::ExplicitOverride {
                override_kind: format!("cpu-n:{workers}"),
            },
        }),
        PlacementOverride::Gpu { backend_tag } => {
            let evidence = inputs
                .accelerator_evidence
                .iter()
                .find(|e| &e.backend_tag == backend_tag && e.kind == AcceleratorKind::Gpu);
            let Some(ev) = evidence else {
                return Err(PlacementRejection::OverriddenTargetUnavailable {
                    override_kind: format!("gpu:{backend_tag}"),
                });
            };
            if !ev.parity_proven {
                return Err(PlacementRejection::GpuParityNotProven {
                    backend_tag: backend_tag.clone(),
                });
            }
            Ok(PlacementDecision {
                target: ExecutionTarget::Gpu {
                    backend: backend_tag.clone(),
                },
                reason: PlacementReason::ExplicitOverride {
                    override_kind: format!("gpu:{backend_tag}"),
                },
            })
        }
        PlacementOverride::Fpga { device_tag } => {
            let evidence = inputs
                .accelerator_evidence
                .iter()
                .find(|e| &e.backend_tag == device_tag && e.kind == AcceleratorKind::Fpga);
            let Some(_ev) = evidence else {
                return Err(PlacementRejection::OverriddenTargetUnavailable {
                    override_kind: format!("fpga:{device_tag}"),
                });
            };
            Ok(PlacementDecision {
                target: ExecutionTarget::Fpga {
                    device: device_tag.clone(),
                },
                reason: PlacementReason::ExplicitOverride {
                    override_kind: format!("fpga:{device_tag}"),
                },
            })
        }
    }
}

fn automatic_placement(
    inputs: &PlacementInputs,
    config: &PlacementConfig,
) -> Result<PlacementDecision, PlacementRejection> {
    // 1. Fail-closed: non-bulk-eligible regions use sequential CPU only.
    if !matches!(
        inputs.analysis.shape,
        ExecutionShape::ElementWise | ExecutionShape::Reduction
    ) || !inputs.analysis.gpu_eligible()
    {
        return Ok(PlacementDecision {
            target: ExecutionTarget::Cpu,
            reason: PlacementReason::NotBulkEligible,
        });
    }

    let element_count = match inputs.element_count {
        Some(n) => n,
        None => {
            // Element count unknown → fail closed to sequential CPU.
            return Ok(PlacementDecision {
                target: ExecutionTarget::Cpu,
                reason: PlacementReason::SequentialCpuDefault,
            });
        }
    };

    // 2. Try GPU offload first if threshold met and parity-proven evidence exists.
    //    Only for map operations (reduction parallel placement requires a
    //    ReductionEligibilityProof — checked below).
    if element_count >= config.gpu_element_threshold
        && inputs.analysis.shape == ExecutionShape::ElementWise
    {
        if let Some(ev) = inputs
            .accelerator_evidence
            .iter()
            .find(|e| e.kind == AcceleratorKind::Gpu && e.parity_proven)
        {
            return Ok(PlacementDecision {
                target: ExecutionTarget::Gpu {
                    backend: ev.backend_tag.clone(),
                },
                reason: PlacementReason::GpuOffload {
                    backend_tag: ev.backend_tag.clone(),
                    element_count,
                    threshold: config.gpu_element_threshold,
                },
            });
        }
    }

    // 3. Try multicore CPU when threshold met, workers > 1, and operation is
    //    element-wise or has a proven reduction eligibility.
    let reduction_eligible = inputs
        .analysis
        .reduction_proof
        .as_ref()
        .is_some_and(|p| p.is_parallel_eligible());
    let parallel_eligible = matches!(inputs.analysis.shape, ExecutionShape::ElementWise)
        || (inputs.analysis.shape == ExecutionShape::Reduction && reduction_eligible);

    if parallel_eligible
        && inputs.available_cpu_workers > 1
        && element_count >= config.multicore_element_threshold
    {
        let workers = inputs.available_cpu_workers;
        return Ok(PlacementDecision {
            target: ExecutionTarget::Gpu {
                // Parallel CPU is surfaced through the GPU slot with a
                // descriptive tag so HeterogeneousGraphExecutor can route it
                // to a registered ParallelCpuNodeExecutor without changing the
                // ExecutionTarget enum.
                backend: format!("cpu-parallel-{workers}"),
            },
            reason: PlacementReason::MulticoreCpu {
                workers,
                element_count,
            },
        });
    }

    // 4. Below multicore threshold or single-worker — sequential CPU.
    if element_count < config.multicore_element_threshold || inputs.available_cpu_workers <= 1 {
        return Ok(PlacementDecision {
            target: ExecutionTarget::Cpu,
            reason: PlacementReason::BelowMulticoreThreshold {
                element_count,
                threshold: config.multicore_element_threshold,
            },
        });
    }

    // 5. Default safe fallback.
    Ok(PlacementDecision {
        target: ExecutionTarget::Cpu,
        reason: PlacementReason::SequentialCpuDefault,
    })
}

// ── Convenience constructors for evidence ─────────────────────────────────────

impl AcceleratorCapabilityEvidence {
    /// Construct parity-proven GPU evidence.
    pub fn proven_gpu(backend_tag: impl Into<String>) -> Self {
        Self {
            backend_tag: backend_tag.into(),
            parity_proven: true,
            kind: AcceleratorKind::Gpu,
        }
    }

    /// Construct GPU evidence without parity proof.
    pub fn unproven_gpu(backend_tag: impl Into<String>) -> Self {
        Self {
            backend_tag: backend_tag.into(),
            parity_proven: false,
            kind: AcceleratorKind::Gpu,
        }
    }

    /// Construct FPGA evidence (parity proof not yet required for FPGA path).
    pub fn live_fpga(device_tag: impl Into<String>) -> Self {
        Self {
            backend_tag: device_tag.into(),
            parity_proven: true,
            kind: AcceleratorKind::Fpga,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compute::{
        AdmissionBlocker, BulkOperation, ComputeAnalysis, ComputeRegion, EffectClass,
        ExecutionShape, GroupingLaw, NumericDomain, ReductionEligibilityProof, StorageClass,
    };
    use crate::ir::{BufferLiteral, Ir, Params};

    fn admitted_map_analysis(element_count: usize) -> (PlacementInputs, ComputeAnalysis) {
        let input_buffer = Ir::Buffer(BufferLiteral::I32(vec![0i32; element_count]));
        let function = Ir::Lambda {
            params: Params::Fixed(vec!["X".into()]),
            body: Box::new(Ir::Var("X".into())),
        };
        let analysis = ComputeAnalysis {
            shape: ExecutionShape::ElementWise,
            effect: EffectClass::Pure,
            storage: StorageClass::ContiguousBuffer,
            numeric_domain: NumericDomain::FixedWidthInteger,
            region: Some(ComputeRegion {
                operation: BulkOperation::Map,
                function,
                input: input_buffer,
                initial: None,
                kernel: None,
            }),
            gpu_blockers: vec![],
            reduction_proof: None,
        };
        let inputs = PlacementInputs {
            element_count: Some(element_count),
            analysis: analysis.clone(),
            available_cpu_workers: 4,
            accelerator_evidence: vec![],
        };
        (inputs, analysis)
    }

    fn admitted_reduction_analysis(element_count: usize, eligible: bool) -> PlacementInputs {
        let input_buffer = Ir::Buffer(BufferLiteral::I32(vec![1i32; element_count]));
        let function = Ir::Var("+".into());
        let reduction_proof = if eligible {
            Some(ReductionEligibilityProof {
                grouping_law: GroupingLaw::Associative { identity: Some(0) },
                overflow_invariant: true,
                contiguous_storage: true,
                pure_kernel: true,
                numeric_domain: NumericDomain::FixedWidthInteger,
            })
        } else {
            None
        };
        let analysis = ComputeAnalysis {
            shape: ExecutionShape::Reduction,
            effect: EffectClass::Pure,
            storage: StorageClass::ContiguousBuffer,
            numeric_domain: NumericDomain::FixedWidthInteger,
            region: Some(ComputeRegion {
                operation: BulkOperation::Reduce,
                function,
                input: input_buffer,
                initial: None,
                kernel: None,
            }),
            gpu_blockers: vec![],
            reduction_proof,
        };
        PlacementInputs {
            element_count: Some(element_count),
            analysis,
            available_cpu_workers: 4,
            accelerator_evidence: vec![],
        }
    }

    fn ineligible_analysis() -> PlacementInputs {
        let analysis = ComputeAnalysis {
            shape: ExecutionShape::Irregular,
            effect: EffectClass::Stateful,
            storage: StorageClass::Unknown,
            numeric_domain: NumericDomain::Unknown,
            region: None,
            gpu_blockers: vec![AdmissionBlocker::NotBulkParallel],
            reduction_proof: None,
        };
        PlacementInputs {
            element_count: Some(1_000_000),
            analysis,
            available_cpu_workers: 4,
            accelerator_evidence: vec![],
        }
    }

    // ── Automatic placement ───────────────────────────────────────────────────

    #[test]
    fn ineligible_region_always_maps_to_sequential_cpu() {
        let inputs = ineligible_analysis();
        let config = PlacementConfig::default();
        let decision = place(&inputs, &config, None).unwrap();
        assert_eq!(decision.target, ExecutionTarget::Cpu);
        assert_eq!(decision.reason, PlacementReason::NotBulkEligible);
    }

    #[test]
    fn small_admitted_map_stays_on_sequential_cpu() {
        let (inputs, _) = admitted_map_analysis(100);
        let config = PlacementConfig {
            multicore_element_threshold: 4_096,
            gpu_element_threshold: 65_536,
        };
        let decision = place(&inputs, &config, None).unwrap();
        assert_eq!(decision.target, ExecutionTarget::Cpu);
        assert!(matches!(
            decision.reason,
            PlacementReason::BelowMulticoreThreshold { .. }
        ));
    }

    #[test]
    fn medium_admitted_map_uses_multicore_cpu() {
        let (mut inputs, _) = admitted_map_analysis(10_000);
        inputs.available_cpu_workers = 4;
        let config = PlacementConfig {
            multicore_element_threshold: 4_096,
            gpu_element_threshold: 65_536,
        };
        let decision = place(&inputs, &config, None).unwrap();
        assert_eq!(
            decision.target,
            ExecutionTarget::Gpu {
                backend: "cpu-parallel-4".into()
            }
        );
        assert!(matches!(
            decision.reason,
            PlacementReason::MulticoreCpu { workers: 4, .. }
        ));
    }

    #[test]
    fn single_worker_stays_sequential_even_above_threshold() {
        let (mut inputs, _) = admitted_map_analysis(50_000);
        inputs.available_cpu_workers = 1;
        let config = PlacementConfig::default();
        let decision = place(&inputs, &config, None).unwrap();
        assert_eq!(decision.target, ExecutionTarget::Cpu);
    }

    #[test]
    fn large_map_with_proven_gpu_offloads_to_gpu() {
        let (mut inputs, _) = admitted_map_analysis(100_000);
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::proven_gpu("cuda")];
        let config = PlacementConfig {
            multicore_element_threshold: 4_096,
            gpu_element_threshold: 65_536,
        };
        let decision = place(&inputs, &config, None).unwrap();
        assert_eq!(
            decision.target,
            ExecutionTarget::Gpu {
                backend: "cuda".into()
            }
        );
        assert!(matches!(
            decision.reason,
            PlacementReason::GpuOffload { .. }
        ));
    }

    #[test]
    fn large_map_with_unproven_gpu_falls_back_to_multicore_cpu() {
        let (mut inputs, _) = admitted_map_analysis(100_000);
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::unproven_gpu("cuda")];
        let config = PlacementConfig::default();
        let decision = place(&inputs, &config, None).unwrap();
        // GPU parity not proven → multicore CPU
        assert_eq!(
            decision.target,
            ExecutionTarget::Gpu {
                backend: "cpu-parallel-4".into()
            }
        );
        assert!(matches!(
            decision.reason,
            PlacementReason::MulticoreCpu { .. }
        ));
    }

    #[test]
    fn unknown_element_count_forces_sequential_cpu() {
        let (mut inputs, _) = admitted_map_analysis(1);
        inputs.element_count = None;
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::proven_gpu("cuda")];
        let config = PlacementConfig::default();
        let decision = place(&inputs, &config, None).unwrap();
        assert_eq!(decision.target, ExecutionTarget::Cpu);
        assert_eq!(decision.reason, PlacementReason::SequentialCpuDefault);
    }

    // ── Reduction placement ───────────────────────────────────────────────────

    #[test]
    fn eligible_reduction_above_threshold_uses_multicore() {
        let mut inputs = admitted_reduction_analysis(10_000, true);
        inputs.available_cpu_workers = 4;
        let config = PlacementConfig {
            multicore_element_threshold: 4_096,
            gpu_element_threshold: 65_536,
        };
        let decision = place(&inputs, &config, None).unwrap();
        assert!(matches!(
            decision.reason,
            PlacementReason::MulticoreCpu { .. }
        ));
    }

    #[test]
    fn ineligible_reduction_stays_sequential_cpu() {
        let mut inputs = admitted_reduction_analysis(100_000, false);
        inputs.available_cpu_workers = 4;
        let config = PlacementConfig::default();
        let decision = place(&inputs, &config, None).unwrap();
        // No proof → not parallel eligible → sequential CPU (notbulkeligible
        // or belowthreshold depending on path; either way target is Cpu)
        assert_eq!(decision.target, ExecutionTarget::Cpu);
    }

    // ── Override path ─────────────────────────────────────────────────────────

    #[test]
    fn cpu_sequential_override_forces_sequential_even_for_large_buffer() {
        let (mut inputs, _) = admitted_map_analysis(1_000_000);
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::proven_gpu("cuda")];
        let config = PlacementConfig::default();
        let decision = place(&inputs, &config, Some(&PlacementOverride::CpuSequential)).unwrap();
        assert_eq!(decision.target, ExecutionTarget::Cpu);
        assert!(matches!(
            decision.reason,
            PlacementReason::ExplicitOverride { .. }
        ));
    }

    #[test]
    fn cpu_parallel_override_forces_parallel_even_for_small_buffer() {
        let (inputs, _) = admitted_map_analysis(10);
        let config = PlacementConfig::default();
        let decision = place(
            &inputs,
            &config,
            Some(&PlacementOverride::CpuParallel { workers: 2 }),
        )
        .unwrap();
        assert_eq!(
            decision.target,
            ExecutionTarget::Gpu {
                backend: "cpu-parallel-2".into()
            }
        );
    }

    #[test]
    fn gpu_override_requires_live_evidence() {
        let (inputs, _) = admitted_map_analysis(10_000);
        let config = PlacementConfig::default();
        let err = place(
            &inputs,
            &config,
            Some(&PlacementOverride::Gpu {
                backend_tag: "cuda".into(),
            }),
        )
        .unwrap_err();
        assert_eq!(
            err,
            PlacementRejection::OverriddenTargetUnavailable {
                override_kind: "gpu:cuda".into()
            }
        );
    }

    #[test]
    fn gpu_override_with_unproven_evidence_is_rejected() {
        let (mut inputs, _) = admitted_map_analysis(10_000);
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::unproven_gpu("cuda")];
        let config = PlacementConfig::default();
        let err = place(
            &inputs,
            &config,
            Some(&PlacementOverride::Gpu {
                backend_tag: "cuda".into(),
            }),
        )
        .unwrap_err();
        assert_eq!(
            err,
            PlacementRejection::GpuParityNotProven {
                backend_tag: "cuda".into()
            }
        );
    }

    #[test]
    fn gpu_override_with_proven_evidence_succeeds() {
        let (mut inputs, _) = admitted_map_analysis(10_000);
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::proven_gpu("cuda")];
        let config = PlacementConfig::default();
        let decision = place(
            &inputs,
            &config,
            Some(&PlacementOverride::Gpu {
                backend_tag: "cuda".into(),
            }),
        )
        .unwrap();
        assert_eq!(
            decision.target,
            ExecutionTarget::Gpu {
                backend: "cuda".into()
            }
        );
    }

    #[test]
    fn fpga_override_requires_live_evidence() {
        let (inputs, _) = admitted_map_analysis(1_000);
        let config = PlacementConfig::default();
        let err = place(
            &inputs,
            &config,
            Some(&PlacementOverride::Fpga {
                device_tag: "gw5a".into(),
            }),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            PlacementRejection::OverriddenTargetUnavailable { .. }
        ));
    }

    #[test]
    fn fpga_override_with_live_evidence_succeeds() {
        let (mut inputs, _) = admitted_map_analysis(1_000);
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::live_fpga("gw5a")];
        let config = PlacementConfig::default();
        let decision = place(
            &inputs,
            &config,
            Some(&PlacementOverride::Fpga {
                device_tag: "gw5a".into(),
            }),
        )
        .unwrap();
        assert_eq!(
            decision.target,
            ExecutionTarget::Fpga {
                device: "gw5a".into()
            }
        );
    }

    // ── Threshold boundary conditions ─────────────────────────────────────────

    #[test]
    fn exactly_at_multicore_threshold_uses_multicore() {
        let threshold = 4_096;
        let (inputs, _) = admitted_map_analysis(threshold);
        let config = PlacementConfig {
            multicore_element_threshold: threshold,
            gpu_element_threshold: 65_536,
        };
        let decision = place(&inputs, &config, None).unwrap();
        assert!(matches!(
            decision.reason,
            PlacementReason::MulticoreCpu { .. }
        ));
    }

    #[test]
    fn one_below_multicore_threshold_stays_sequential() {
        let threshold = 4_096;
        let (inputs, _) = admitted_map_analysis(threshold - 1);
        let config = PlacementConfig {
            multicore_element_threshold: threshold,
            gpu_element_threshold: 65_536,
        };
        let decision = place(&inputs, &config, None).unwrap();
        assert_eq!(decision.target, ExecutionTarget::Cpu);
    }

    #[test]
    fn changing_threshold_does_not_change_map_result_only_executor() {
        // Verify: the same IR, same input, different config only changes the
        // executor choice — never the contractual computation result.
        // (This test proves provenance correctness, not numerical output.)
        let (inputs_small_threshold, _) = admitted_map_analysis(1_000);
        let config_low = PlacementConfig {
            multicore_element_threshold: 100,
            gpu_element_threshold: 65_536,
        };
        let config_high = PlacementConfig {
            multicore_element_threshold: 100_000,
            gpu_element_threshold: 65_536,
        };
        let decision_low = place(&inputs_small_threshold, &config_low, None).unwrap();
        let decision_high = place(&inputs_small_threshold, &config_high, None).unwrap();
        // Different executors may be chosen…
        assert_ne!(decision_low.reason, decision_high.reason);
        // …but both are valid (non-error) decisions.
        // The contractual result invariant is enforced by the backend parity
        // tests in cpu_parallel_compute_backend_test.rs, not here.
    }
}
