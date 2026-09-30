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
//! - Changing `PlacementConfig` cost coefficients or CPU parallel threshold
//!   **never changes the contractual result** of a semantically admitted
//!   computation — only the physical executor chosen.

use crate::compute::{BulkOperation, ComputeAnalysis, ExecutionShape, ScalarExpr};
use crate::execution::ExecutionTarget;
use crate::ir::{BufferLiteral, Ir};

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
    /// The map input already lives on the candidate GPU, so no HtoD transfer
    /// is required for this placement decision. This is mechanism state only.
    pub gpu_input_resident: bool,
    /// The caller can keep the produced buffer resident on the candidate GPU,
    /// so no immediate DtoH materialization is required.
    pub gpu_output_can_remain_resident: bool,
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

/// Integer fixed-point coefficients for mechanism cost estimation.
///
/// Every field uses the same caller-defined cost unit. The planner only
/// compares totals, so the unit may be calibrated nanoseconds, picoseconds, or
/// another fixed-point scale. No coefficient is language semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementCostProfile {
    /// Fixed CPU dispatch/scheduling cost.
    pub cpu_dispatch_cost: u64,
    /// Per-element CPU memory/loop cost before arithmetic.
    pub cpu_element_cost: u64,
    /// Per scalar arithmetic operation and element on CPU.
    pub cpu_operation_cost: u64,
    /// Fixed GPU launch cost.
    pub gpu_launch_cost: u64,
    /// Per-element GPU memory/loop cost before arithmetic.
    pub gpu_element_cost: u64,
    /// Per scalar arithmetic operation and element on GPU.
    pub gpu_operation_cost: u64,
    /// Per-byte host-to-device transfer cost.
    pub h2d_byte_cost: u64,
    /// Per-byte device-to-host transfer cost.
    pub dtoh_byte_cost: u64,
}

impl Default for PlacementCostProfile {
    fn default() -> Self {
        // Conservative synthetic baseline. It intentionally does not claim to
        // describe every machine; owner-hardware calibration may replace it.
        Self {
            cpu_dispatch_cost: 20_000,
            cpu_element_cost: 640,
            cpu_operation_cost: 320,
            gpu_launch_cost: 3_000_000,
            gpu_element_cost: 40,
            gpu_operation_cost: 40,
            h2d_byte_cost: 4,
            dtoh_byte_cost: 4,
        }
    }
}

/// Mechanism-side transfer estimate for one map placement candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementTransferEstimate {
    pub h2d_bytes: u64,
    pub dtoh_bytes: u64,
}

/// Reproducible cost evidence used to choose between CPU and GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementCostEstimate {
    pub scalar_operation_count: u64,
    pub transfers: PlacementTransferEstimate,
    pub cpu_workers: usize,
    pub cpu_cost: u64,
    pub gpu_cost: u64,
}

/// Calibrated mechanism/configuration for placement.
///
/// The multicore threshold only decides whether the CPU estimate may use more
/// than one worker. GPU placement itself is selected by the explicit cost
/// comparison in PlacementCostProfile, not by an element-count threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementConfig {
    pub multicore_element_threshold: usize,
    pub cost_profile: PlacementCostProfile,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self {
            multicore_element_threshold: 4_096,
            cost_profile: PlacementCostProfile::default(),
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
    /// A parity-proven GPU was considered, but the calibrated CPU estimate was
    /// no greater. Both estimates are retained as placement provenance.
    CpuCostPreferred {
        workers: usize,
        element_count: usize,
        backend_tag: String,
        estimate: PlacementCostEstimate,
    },
    /// A parity-proven GPU had the lower calibrated mechanism cost.
    GpuOffload {
        backend_tag: String,
        element_count: usize,
        estimate: PlacementCostEstimate,
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

/// Count scalar arithmetic operations in an admitted kernel expression.
///
/// Parameters and constants are leaves; CheckedAdd is one operation plus its
/// children. Saturation keeps pathological trees deterministic.
pub fn scalar_operation_count(expression: &ScalarExpr) -> u64 {
    match expression {
        ScalarExpr::Parameter(_) | ScalarExpr::ExactInteger(_) | ScalarExpr::Float32(_) => 0,
        ScalarExpr::CheckedAdd(left, right) => 1u64
            .saturating_add(scalar_operation_count(left))
            .saturating_add(scalar_operation_count(right)),
        // GPU-2 #368: multiplication counts like CheckedAdd -- one
        // arithmetic operation plus children; a float constant is a leaf.
        ScalarExpr::Mul(left, right) => 1u64
            .saturating_add(scalar_operation_count(left))
            .saturating_add(scalar_operation_count(right)),
    }
}

fn input_buffer_shape(input: &Ir) -> Option<(usize, u64)> {
    match input {
        Ir::Buffer(BufferLiteral::I32(values)) => Some((values.len(), 4)),
        Ir::Buffer(BufferLiteral::F32(values)) => Some((values.len(), 4)),
        _ => None,
    }
}

/// Estimate host/device traffic from explicit residency facts.
pub fn estimate_map_transfer_bytes(inputs: &PlacementInputs) -> Option<PlacementTransferEstimate> {
    let element_count = inputs.element_count?;
    let region = inputs.analysis.region.as_ref()?;
    if region.operation != BulkOperation::Map {
        return None;
    }
    let (buffer_len, element_width) = input_buffer_shape(&region.input)?;
    if buffer_len != element_count {
        return None;
    }
    let buffer_bytes = u64::try_from(element_count)
        .ok()?
        .checked_mul(element_width)?;
    Some(PlacementTransferEstimate {
        h2d_bytes: if inputs.gpu_input_resident {
            0
        } else {
            buffer_bytes
        },
        dtoh_bytes: if inputs.gpu_output_can_remain_resident {
            0
        } else {
            buffer_bytes
        },
    })
}

fn estimated_cpu_workers(inputs: &PlacementInputs, config: &PlacementConfig) -> usize {
    let element_count = inputs.element_count.unwrap_or(0);
    if inputs.available_cpu_workers > 1 && element_count >= config.multicore_element_threshold {
        inputs.available_cpu_workers
    } else {
        1
    }
}

fn ceil_div_u64(value: u64, divisor: u64) -> u64 {
    debug_assert!(divisor > 0);
    value.saturating_add(divisor - 1) / divisor
}

/// Estimate CPU and GPU mechanism cost for an admitted element-wise map.
///
/// Returns None when the required facts are incomplete. That is a fail-closed
/// signal: automatic placement must not invent a GPU estimate.
pub fn estimate_map_costs(
    inputs: &PlacementInputs,
    config: &PlacementConfig,
) -> Option<PlacementCostEstimate> {
    let element_count = u64::try_from(inputs.element_count?).ok()?;
    let region = inputs.analysis.region.as_ref()?;
    if region.operation != BulkOperation::Map {
        return None;
    }
    let kernel = region.kernel.as_ref()?;
    let scalar_operation_count = scalar_operation_count(&kernel.body);
    let transfers = estimate_map_transfer_bytes(inputs)?;
    let workers = estimated_cpu_workers(inputs, config).max(1);
    let profile = &config.cost_profile;

    let cpu_per_element = profile.cpu_element_cost.saturating_add(
        profile
            .cpu_operation_cost
            .saturating_mul(scalar_operation_count),
    );
    let cpu_work = element_count.saturating_mul(cpu_per_element);
    let cpu_cost = profile
        .cpu_dispatch_cost
        .saturating_add(ceil_div_u64(cpu_work, u64::try_from(workers).ok()?));

    let gpu_per_element = profile.gpu_element_cost.saturating_add(
        profile
            .gpu_operation_cost
            .saturating_mul(scalar_operation_count),
    );
    let gpu_cost = profile
        .gpu_launch_cost
        .saturating_add(element_count.saturating_mul(gpu_per_element))
        .saturating_add(transfers.h2d_bytes.saturating_mul(profile.h2d_byte_cost))
        .saturating_add(transfers.dtoh_bytes.saturating_mul(profile.dtoh_byte_cost));

    Some(PlacementCostEstimate {
        scalar_operation_count,
        transfers,
        cpu_workers: workers,
        cpu_cost,
        gpu_cost,
    })
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
/// 2. If a parity-proven GPU and complete cost facts exist, compare calibrated
///    CPU and GPU mechanism costs. Ties stay on CPU.
/// 3. Without a complete GPU comparison, use the existing conservative CPU
///    sequential/multicore policy.
/// 4. Sequential CPU remains the safe fallback.
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

    // 2. Compare explicit mechanism costs when a parity-proven GPU is present.
    //    Missing kernel/transfer facts are fail-closed: automatic GPU placement
    //    is skipped instead of guessing.
    if inputs.analysis.shape == ExecutionShape::ElementWise {
        if let Some(ev) = inputs
            .accelerator_evidence
            .iter()
            .find(|e| e.kind == AcceleratorKind::Gpu && e.parity_proven)
        {
            if let Some(estimate) = estimate_map_costs(inputs, config) {
                if estimate.gpu_cost < estimate.cpu_cost {
                    return Ok(PlacementDecision {
                        target: ExecutionTarget::Gpu {
                            backend: ev.backend_tag.clone(),
                        },
                        reason: PlacementReason::GpuOffload {
                            backend_tag: ev.backend_tag.clone(),
                            element_count,
                            estimate,
                        },
                    });
                }

                let workers = estimate.cpu_workers;
                let target = if workers > 1 {
                    ExecutionTarget::Gpu {
                        backend: format!("cpu-parallel-{workers}"),
                    }
                } else {
                    ExecutionTarget::Cpu
                };
                return Ok(PlacementDecision {
                    target,
                    reason: PlacementReason::CpuCostPreferred {
                        workers,
                        element_count,
                        backend_tag: ev.backend_tag.clone(),
                        estimate,
                    },
                });
            }
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
        AdmissionBlocker, BulkOperation, ComputeAnalysis, ComputeKernel, ComputeRegion,
        EffectClass, ExecutionShape, GroupingLaw, NumericDomain, ReductionEligibilityProof,
        ScalarExpr, StorageClass,
    };
    use crate::ir::{BufferLiteral, Ir, Params};

    fn admitted_map_analysis(element_count: usize) -> (PlacementInputs, ComputeAnalysis) {
        let input_buffer = Ir::Buffer(BufferLiteral::I32(vec![0i32; element_count]));
        let function = Ir::Lambda {
            params: Params::Fixed(vec!["X".into()]),
            body: Box::new(Ir::App {
                func: Box::new(Ir::Sid(sens::sens!(00001100))),
                args: vec![Ir::Var("X".into()), Ir::Int(1)],
            }),
        };
        let analysis = ComputeAnalysis {
            shape: ExecutionShape::ElementWise,
            effect: EffectClass::Pure,
            storage: StorageClass::ContiguousBuffer,
            numeric_domain: NumericDomain::FixedWidthInteger,
            region: Some(ComputeRegion {
                identity: sens::sens!(01011001),
                operation: BulkOperation::Map,
                function,
                input: input_buffer,
                initial: None,
                kernel: Some(ComputeKernel {
                    parameter_count: 1,
                    body: ScalarExpr::CheckedAdd(
                        Box::new(ScalarExpr::Parameter(0)),
                        Box::new(ScalarExpr::ExactInteger(1)),
                    ),
                }),
            }),
            gpu_blockers: vec![],
            reduction_proof: None,
        };
        let inputs = PlacementInputs {
            element_count: Some(element_count),
            analysis: analysis.clone(),
            available_cpu_workers: 4,
            accelerator_evidence: vec![],
            gpu_input_resident: false,
            gpu_output_can_remain_resident: false,
        };
        (inputs, analysis)
    }

    fn checked_add_chain(operation_count: usize) -> ScalarExpr {
        (0..operation_count).fold(ScalarExpr::Parameter(0), |expr, _| {
            ScalarExpr::CheckedAdd(Box::new(expr), Box::new(ScalarExpr::ExactInteger(1)))
        })
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
                identity: sens::sens!(00111001),
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
            gpu_input_resident: false,
            gpu_output_can_remain_resident: false,
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
            gpu_input_resident: false,
            gpu_output_can_remain_resident: false,
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
            cost_profile: PlacementCostProfile::default(),
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
            cost_profile: PlacementCostProfile::default(),
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
            cost_profile: PlacementCostProfile::default(),
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
            cost_profile: PlacementCostProfile::default(),
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

    // ── Cost model evidence ─────────────────────────────────────────────────

    #[test]
    fn scalar_operation_count_walks_nested_checked_add() {
        let expression = ScalarExpr::CheckedAdd(
            Box::new(ScalarExpr::CheckedAdd(
                Box::new(ScalarExpr::Parameter(0)),
                Box::new(ScalarExpr::ExactInteger(1)),
            )),
            Box::new(ScalarExpr::CheckedAdd(
                Box::new(ScalarExpr::ExactInteger(2)),
                Box::new(ScalarExpr::ExactInteger(3)),
            )),
        );
        assert_eq!(scalar_operation_count(&expression), 3);
    }

    #[test]
    fn transfer_estimate_respects_explicit_residency() {
        let (mut inputs, _) = admitted_map_analysis(10);
        assert_eq!(
            estimate_map_transfer_bytes(&inputs),
            Some(PlacementTransferEstimate {
                h2d_bytes: 40,
                dtoh_bytes: 40,
            })
        );

        inputs.gpu_input_resident = true;
        assert_eq!(
            estimate_map_transfer_bytes(&inputs),
            Some(PlacementTransferEstimate {
                h2d_bytes: 0,
                dtoh_bytes: 40,
            })
        );

        inputs.gpu_output_can_remain_resident = true;
        assert_eq!(
            estimate_map_transfer_bytes(&inputs),
            Some(PlacementTransferEstimate {
                h2d_bytes: 0,
                dtoh_bytes: 0,
            })
        );
    }

    #[test]
    fn cpu_cost_preference_records_both_estimates() {
        let (mut inputs, _) = admitted_map_analysis(10_000);
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::proven_gpu("cuda")];
        let decision = place(&inputs, &PlacementConfig::default(), None).unwrap();
        assert_eq!(
            decision.target,
            ExecutionTarget::Gpu {
                backend: "cpu-parallel-4".into()
            }
        );
        let PlacementReason::CpuCostPreferred { estimate, .. } = decision.reason else {
            panic!("expected calibrated CPU preference");
        };
        assert_eq!(estimate.scalar_operation_count, 1);
        assert_eq!(estimate.transfers.h2d_bytes, 40_000);
        assert_eq!(estimate.transfers.dtoh_bytes, 40_000);
        assert_eq!(estimate.cpu_workers, 4);
        assert!(estimate.cpu_cost < estimate.gpu_cost);
    }

    #[test]
    fn residency_can_flip_cpu_cost_preference_to_gpu() {
        let (mut inputs, _) = admitted_map_analysis(20_000);
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::proven_gpu("cuda")];
        let config = PlacementConfig::default();

        let host_roundtrip = place(&inputs, &config, None).unwrap();
        assert!(matches!(
            host_roundtrip.reason,
            PlacementReason::CpuCostPreferred { .. }
        ));

        inputs.gpu_input_resident = true;
        inputs.gpu_output_can_remain_resident = true;
        let resident = place(&inputs, &config, None).unwrap();
        assert!(matches!(
            resident.reason,
            PlacementReason::GpuOffload { .. }
        ));
    }

    #[test]
    fn operation_intensity_changes_the_cost_decision() {
        let (mut inputs, _) = admitted_map_analysis(10_000);
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::proven_gpu("cuda")];
        let config = PlacementConfig::default();

        let one_op = place(&inputs, &config, None).unwrap();
        assert!(matches!(
            one_op.reason,
            PlacementReason::CpuCostPreferred { .. }
        ));

        inputs
            .analysis
            .region
            .as_mut()
            .unwrap()
            .kernel
            .as_mut()
            .unwrap()
            .body = checked_add_chain(8);
        let eight_ops = place(&inputs, &config, None).unwrap();
        let PlacementReason::GpuOffload { estimate, .. } = eight_ops.reason else {
            panic!("expected higher arithmetic intensity to prefer GPU");
        };
        assert_eq!(estimate.scalar_operation_count, 8);
        assert!(estimate.gpu_cost < estimate.cpu_cost);
    }

    #[test]
    fn equal_estimated_costs_fail_closed_to_cpu() {
        let (mut inputs, _) = admitted_map_analysis(100_000);
        inputs.accelerator_evidence = vec![AcceleratorCapabilityEvidence::proven_gpu("cuda")];
        let config = PlacementConfig {
            multicore_element_threshold: 4_096,
            cost_profile: PlacementCostProfile {
                cpu_dispatch_cost: 0,
                cpu_element_cost: 0,
                cpu_operation_cost: 0,
                gpu_launch_cost: 0,
                gpu_element_cost: 0,
                gpu_operation_cost: 0,
                h2d_byte_cost: 0,
                dtoh_byte_cost: 0,
            },
        };
        let decision = place(&inputs, &config, None).unwrap();
        assert!(matches!(
            decision.reason,
            PlacementReason::CpuCostPreferred { .. }
        ));
    }
    // ── Threshold boundary conditions ─────────────────────────────────────────

    #[test]
    fn exactly_at_multicore_threshold_uses_multicore() {
        let threshold = 4_096;
        let (inputs, _) = admitted_map_analysis(threshold);
        let config = PlacementConfig {
            multicore_element_threshold: threshold,
            cost_profile: PlacementCostProfile::default(),
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
            cost_profile: PlacementCostProfile::default(),
        };
        let decision = place(&inputs, &config, None).unwrap();
        assert_eq!(decision.target, ExecutionTarget::Cpu);
    }

    #[test]
    fn changing_cpu_parallel_threshold_does_not_change_map_result_only_executor() {
        // Verify: the same IR, same input, different config only changes the
        // executor choice — never the contractual computation result.
        // (This test proves provenance correctness, not numerical output.)
        let (inputs_small_threshold, _) = admitted_map_analysis(1_000);
        let config_low = PlacementConfig {
            multicore_element_threshold: 100,
            cost_profile: PlacementCostProfile::default(),
        };
        let config_high = PlacementConfig {
            multicore_element_threshold: 100_000,
            cost_profile: PlacementCostProfile::default(),
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
