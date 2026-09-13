//! Fail-closed heterogeneous-compute analysis.
//!
//! This is deliberately an analysis layer, not a GPU backend. It recognizes
//! bulk computation in semantic IR, records the representation facts a
//! backend would need, and refuses GPU admission while any fact is unknown.

use crate::ir::{BufferLiteral, Ir, Params, PrimOp, Quoted};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionShape {
    Scalar,
    ElementWise,
    Reduction,
    Irregular,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectClass {
    Pure,
    Allocating,
    Stateful,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageClass {
    Scalar,
    LinkedList,
    ContiguousBuffer,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericDomain {
    Exact,
    FixedWidthInteger,
    InexactFloat,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkOperation {
    Map,
    Reduce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionBlocker {
    NotBulkParallel,
    EffectNotPure,
    StorageNotContiguous,
    NumericDomainNotRepresentable,
    KernelNotLowerable,
    IntegerOverflowNotProven,
    FloatRoundingNotDefined,
}

/// Backend-neutral scalar subset allowed inside a bulk kernel. Every node is
/// pure, allocation-free, and has explicit checked-integer semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalarExpr {
    Parameter(usize),
    ExactInteger(i64),
    CheckedAdd(Box<ScalarExpr>, Box<ScalarExpr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeKernel {
    pub parameter_count: usize,
    pub body: ScalarExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputeRegion {
    pub operation: BulkOperation,
    pub function: Ir,
    pub input: Ir,
    pub initial: Option<Ir>,
    pub kernel: Option<ComputeKernel>,
}

/// Machine-readable algebraic grouping law for reduction operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingLaw {
    /// Operation is strictly associative with optional identity: (a op b) op c == a op (b op c).
    Associative { identity: Option<i64> },
    /// Non-associative or grouping-sensitive operation.
    NonAssociative,
}

/// Machine-readable proof determining whether a reduction may be parallelized
/// without any observable departure from sequential reference semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReductionEligibilityProof {
    pub grouping_law: GroupingLaw,
    pub overflow_invariant: bool,
    pub contiguous_storage: bool,
    pub pure_kernel: bool,
    pub numeric_domain: NumericDomain,
}

impl ReductionEligibilityProof {
    pub fn is_parallel_eligible(&self) -> bool {
        matches!(self.grouping_law, GroupingLaw::Associative { .. })
            && self.overflow_invariant
            && self.contiguous_storage
            && self.pure_kernel
            && self.numeric_domain == NumericDomain::FixedWidthInteger
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputeAnalysis {
    pub shape: ExecutionShape,
    pub effect: EffectClass,
    pub storage: StorageClass,
    pub numeric_domain: NumericDomain,
    pub region: Option<ComputeRegion>,
    pub gpu_blockers: Vec<AdmissionBlocker>,
    pub reduction_proof: Option<ReductionEligibilityProof>,
}

impl ComputeAnalysis {
    pub fn gpu_eligible(&self) -> bool {
        self.gpu_blockers.is_empty()
    }
}

/// Analyze semantic IR using only facts already represented in that IR.
/// Unknown representation is a blocker, never an invitation to coerce.
pub fn analyze(ir: &Ir) -> ComputeAnalysis {
    let region = extract_region(ir);
    let shape = match region.as_ref().map(|region| region.operation) {
        Some(BulkOperation::Map) => ExecutionShape::ElementWise,
        Some(BulkOperation::Reduce) => ExecutionShape::Reduction,
        None if is_scalar(ir) => ExecutionShape::Scalar,
        None => ExecutionShape::Irregular,
    };
    let effect = effect_of(ir);
    let storage = region
        .as_ref()
        .map(|region| storage_of(&region.input))
        .unwrap_or(StorageClass::Scalar);
    let numeric_domain = region
        .as_ref()
        .map(|region| numeric_domain_of(&region.input))
        .unwrap_or_else(|| numeric_domain_of(ir));

    let mut gpu_blockers = Vec::new();
    if !matches!(
        shape,
        ExecutionShape::ElementWise | ExecutionShape::Reduction
    ) {
        gpu_blockers.push(AdmissionBlocker::NotBulkParallel);
    }
    if effect != EffectClass::Pure {
        gpu_blockers.push(AdmissionBlocker::EffectNotPure);
    }
    if storage != StorageClass::ContiguousBuffer {
        gpu_blockers.push(AdmissionBlocker::StorageNotContiguous);
    }
    if !matches!(
        numeric_domain,
        NumericDomain::FixedWidthInteger | NumericDomain::InexactFloat
    ) {
        gpu_blockers.push(AdmissionBlocker::NumericDomainNotRepresentable);
    }
    if region
        .as_ref()
        .is_some_and(|region| region.kernel.is_none())
    {
        gpu_blockers.push(AdmissionBlocker::KernelNotLowerable);
    }
    match (numeric_domain, region.as_ref()) {
        (NumericDomain::FixedWidthInteger, Some(region)) if !i32_range_proven(region) => {
            gpu_blockers.push(AdmissionBlocker::IntegerOverflowNotProven);
        }
        (NumericDomain::InexactFloat, Some(region)) if !f32_rounding_proven(region) => {
            gpu_blockers.push(AdmissionBlocker::FloatRoundingNotDefined);
        }
        _ => {}
    }

    let reduction_proof = match (shape, region.as_ref()) {
        (ExecutionShape::Reduction, Some(region)) => {
            prove_reduction_eligibility(region, effect, storage, numeric_domain)
        }
        _ => None,
    };

    ComputeAnalysis {
        shape,
        effect,
        storage,
        numeric_domain,
        region,
        gpu_blockers,
        reduction_proof,
    }
}

fn extract_region(ir: &Ir) -> Option<ComputeRegion> {
    let Ir::App { func, args } = ir else {
        return None;
    };
    let name = match &**func {
        Ir::Var(name) => name,
        Ir::Builtin(name) => name,
        _ => return None,
    };
    match (name.as_str(), args.as_slice()) {
        ("MAP" | "NUMERIC-BUFFER-MAP", [function, input]) => Some(ComputeRegion {
            operation: BulkOperation::Map,
            function: function.clone(),
            input: input.clone(),
            initial: None,
            kernel: lower_kernel(function, 1),
        }),
        ("REDUCE" | "NUMERIC-BUFFER-REDUCE", [function, initial, input]) => Some(ComputeRegion {
            operation: BulkOperation::Reduce,
            function: function.clone(),
            input: input.clone(),
            initial: Some(initial.clone()),
            kernel: lower_kernel(function, 2),
        }),
        _ => None,
    }
}

fn prove_reduction_eligibility(
    region: &ComputeRegion,
    effect: EffectClass,
    storage: StorageClass,
    numeric_domain: NumericDomain,
) -> Option<ReductionEligibilityProof> {
    let Some(kernel) = &region.kernel else {
        return Some(ReductionEligibilityProof {
            grouping_law: GroupingLaw::NonAssociative,
            overflow_invariant: false,
            contiguous_storage: storage == StorageClass::ContiguousBuffer,
            pure_kernel: effect == EffectClass::Pure,
            numeric_domain,
        });
    };
    if kernel.parameter_count != 2 {
        return Some(ReductionEligibilityProof {
            grouping_law: GroupingLaw::NonAssociative,
            overflow_invariant: false,
            contiguous_storage: storage == StorageClass::ContiguousBuffer,
            pure_kernel: effect == EffectClass::Pure,
            numeric_domain,
        });
    }

    let is_associative_add = match &kernel.body {
        ScalarExpr::CheckedAdd(left, right) => match (&**left, &**right) {
            (ScalarExpr::Parameter(0), ScalarExpr::Parameter(1))
            | (ScalarExpr::Parameter(1), ScalarExpr::Parameter(0)) => true,
            _ => false,
        },
        _ => false,
    };

    let grouping_law = if is_associative_add {
        GroupingLaw::Associative { identity: Some(0) }
    } else {
        GroupingLaw::NonAssociative
    };

    let mut overflow_invariant = false;
    if is_associative_add && numeric_domain == NumericDomain::FixedWidthInteger {
        if let (Ir::Buffer(BufferLiteral::I32(input)), Some(Ir::Int(init))) =
            (&region.input, &region.initial)
        {
            let mut sum_abs: i64 = init.abs();
            let mut safe = true;
            for &x in input {
                if let Some(next) = sum_abs.checked_add((x as i64).abs()) {
                    sum_abs = next;
                    if sum_abs > i32::MAX as i64 {
                        safe = false;
                        break;
                    }
                } else {
                    safe = false;
                    break;
                }
            }
            overflow_invariant = safe;
        }
    }

    Some(ReductionEligibilityProof {
        grouping_law,
        overflow_invariant,
        contiguous_storage: storage == StorageClass::ContiguousBuffer,
        pure_kernel: effect == EffectClass::Pure,
        numeric_domain,
    })
}

fn i32_range_proven(region: &ComputeRegion) -> bool {
    let (Some(kernel), Ir::Buffer(BufferLiteral::I32(input))) = (&region.kernel, &region.input)
    else {
        return false;
    };
    match region.operation {
        BulkOperation::Map => input.iter().all(|element| {
            eval_i32_range(&kernel.body, &[i64::from(*element)])
                .is_some_and(|value| i32::try_from(value).is_ok())
        }),
        BulkOperation::Reduce => {
            let Some(Ir::Int(init)) = &region.initial else {
                return false;
            };
            let mut acc = *init;
            if i32::try_from(acc).is_err() {
                return false;
            }
            for &element in input {
                let Some(next) = eval_i32_range(&kernel.body, &[acc, i64::from(element)]) else {
                    return false;
                };
                if i32::try_from(next).is_err() {
                    return false;
                }
                acc = next;
            }
            true
        }
    }
}

fn eval_i32_range(expression: &ScalarExpr, parameters: &[i64]) -> Option<i64> {
    let value = match expression {
        ScalarExpr::Parameter(index) => parameters.get(*index).copied(),
        ScalarExpr::ExactInteger(value) => Some(*value),
        ScalarExpr::CheckedAdd(left, right) => {
            eval_i32_range(left, parameters)?.checked_add(eval_i32_range(right, parameters)?)
        }
    }?;
    i32::try_from(value).ok().map(i64::from)
}

fn f32_rounding_proven(region: &ComputeRegion) -> bool {
    let (Some(kernel), Ir::Buffer(BufferLiteral::F32(input))) = (&region.kernel, &region.input)
    else {
        return false;
    };
    let Some(offset) = f32_affine_offset(&kernel.body) else {
        return false;
    };
    input.iter().all(|bits| {
        let element = f32::from_bits(*bits);
        let Some(canonical) = eval_f64(&kernel.body, &[f64::from(element)]) else {
            return false;
        };
        let backend = element + offset as f32;
        canonical.is_finite()
            && backend.is_finite()
            && (canonical as f32).to_bits() == backend.to_bits()
    })
}

fn eval_f64(expression: &ScalarExpr, parameters: &[f64]) -> Option<f64> {
    match expression {
        ScalarExpr::Parameter(index) => parameters.get(*index).copied(),
        ScalarExpr::ExactInteger(value) => Some(*value as f64),
        ScalarExpr::CheckedAdd(left, right) => {
            Some(eval_f64(left, parameters)? + eval_f64(right, parameters)?)
        }
    }
}

/// Returns C for exactly the affine form `parameter-0 + C`. Addition trees
/// are flattened so the backend performs one binary32 add, matching the
/// evaluator's one final narrowing step.
///
/// This is deliberately narrower than the integer kernel-body path
/// (`emit_i32_expr` in `gpu_cuda.rs`/`emit_wgsl_map_kernel` in the WGSL
/// backend, which admit a general `CheckedAdd`/`ExactInteger` expression
/// tree): float admission is scoped to this one shape because a general
/// binary32 add tree does not have the same single-rounding-step parity
/// with the canonical evaluator that one final `parameter + constant` add
/// does -- widening it needs its own rounding-parity proof (see this
/// module's f32-vs-canonical bit-exactness check above), not a copy of the
/// integer path's admission rule. CML-GPU-CUDA-INT-FLOAT-ADMISSION-DOCS.
pub(crate) fn f32_affine_offset(expression: &ScalarExpr) -> Option<i64> {
    fn collect(expression: &ScalarExpr) -> Option<(u32, i64)> {
        match expression {
            ScalarExpr::Parameter(0) => Some((1, 0)),
            ScalarExpr::Parameter(_) => None,
            ScalarExpr::ExactInteger(value) => Some((0, *value)),
            ScalarExpr::CheckedAdd(left, right) => {
                let (left_parameters, left_constant) = collect(left)?;
                let (right_parameters, right_constant) = collect(right)?;
                Some((
                    left_parameters.checked_add(right_parameters)?,
                    left_constant.checked_add(right_constant)?,
                ))
            }
        }
    }
    let (parameters, constant) = collect(expression)?;
    (parameters == 1).then_some(constant)
}

fn lower_kernel(function: &Ir, expected_parameters: usize) -> Option<ComputeKernel> {
    if expected_parameters == 2 {
        if matches!(function, Ir::Var(name) | Ir::Builtin(name) if name == "+") {
            return Some(ComputeKernel {
                parameter_count: 2,
                body: ScalarExpr::CheckedAdd(
                    Box::new(ScalarExpr::Parameter(0)),
                    Box::new(ScalarExpr::Parameter(1)),
                ),
            });
        }
    }
    let Ir::Lambda {
        params: Params::Fixed(parameters),
        body,
    } = function
    else {
        return None;
    };
    if parameters.len() != expected_parameters {
        return None;
    }
    let body = lower_scalar_expr(body, parameters)?;
    Some(ComputeKernel {
        parameter_count: parameters.len(),
        body,
    })
}

fn lower_scalar_expr(ir: &Ir, parameters: &[String]) -> Option<ScalarExpr> {
    match ir {
        Ir::Int(value) => Some(ScalarExpr::ExactInteger(*value)),
        Ir::Var(name) => parameters
            .iter()
            .position(|parameter| parameter == name)
            .map(ScalarExpr::Parameter),
        Ir::Prim {
            op: PrimOp::Add,
            args,
        } if args.len() == 2 => Some(ScalarExpr::CheckedAdd(
            Box::new(lower_scalar_expr(&args[0], parameters)?),
            Box::new(lower_scalar_expr(&args[1], parameters)?),
        )),
        Ir::App { func, args }
            if matches!(&**func, Ir::Var(name) if name == "+") && args.len() == 2 =>
        {
            Some(ScalarExpr::CheckedAdd(
                Box::new(lower_scalar_expr(&args[0], parameters)?),
                Box::new(lower_scalar_expr(&args[1], parameters)?),
            ))
        }
        _ => None,
    }
}

fn is_scalar(ir: &Ir) -> bool {
    matches!(
        ir,
        Ir::Int(_) | Ir::Nil | Ir::True | Ir::Var(_) | Ir::Prim { .. }
    )
}

fn effect_of(ir: &Ir) -> EffectClass {
    match ir {
        Ir::Int(_)
        | Ir::Buffer(_)
        | Ir::Nil
        | Ir::True
        | Ir::Var(_)
        | Ir::Quote(_)
        | Ir::Builtin(_) => EffectClass::Pure,
        Ir::Lambda { body, .. } => effect_of(body),
        Ir::Prim {
            op: PrimOp::Cons, ..
        } => EffectClass::Allocating,
        Ir::Prim { args, .. } => join_effects(args.iter().map(effect_of)),
        Ir::Cond { branches } => join_effects(
            branches
                .iter()
                .flat_map(|(test, body)| [effect_of(test), effect_of(body)]),
        ),
        Ir::Let { bindings, body } => join_effects(
            bindings
                .iter()
                .map(|(_, value)| effect_of(value))
                .chain([effect_of(body)]),
        ),
        Ir::Def { .. } => EffectClass::Stateful,
        Ir::TailSelfCall { .. } => EffectClass::Stateful,
        Ir::App { func, args } => {
            let known_pure = match &**func {
                Ir::Var(name) | Ir::Builtin(name) => {
                    name == "+" || name == "MAP" || name == "NUMERIC-BUFFER-MAP" || name == "REDUCE"
                }
                _ => false,
            };
            if known_pure {
                join_effects(args.iter().map(effect_of))
            } else {
                EffectClass::Unknown
            }
        }
        _ => EffectClass::Unknown,
    }
}

fn join_effects(effects: impl IntoIterator<Item = EffectClass>) -> EffectClass {
    effects
        .into_iter()
        .fold(EffectClass::Pure, |left, right| match (left, right) {
            (EffectClass::Stateful, _) | (_, EffectClass::Stateful) => EffectClass::Stateful,
            (EffectClass::Unknown, _) | (_, EffectClass::Unknown) => EffectClass::Unknown,
            (EffectClass::Allocating, _) | (_, EffectClass::Allocating) => EffectClass::Allocating,
            _ => EffectClass::Pure,
        })
}

fn storage_of(ir: &Ir) -> StorageClass {
    match ir {
        Ir::Quote(Quoted::List(_) | Quoted::DottedList(_, _)) | Ir::Nil => StorageClass::LinkedList,
        Ir::Int(_) | Ir::True => StorageClass::Scalar,
        Ir::Buffer(_) => StorageClass::ContiguousBuffer,
        // A call named VECTOR is not enough evidence: my-lisp vectors are
        // heterogeneous and mutable.
        _ => StorageClass::Unknown,
    }
}

fn numeric_domain_of(ir: &Ir) -> NumericDomain {
    match ir {
        Ir::Int(_) | Ir::Quote(Quoted::Int(_)) => NumericDomain::Exact,
        Ir::Buffer(BufferLiteral::I32(_)) => NumericDomain::FixedWidthInteger,
        Ir::Buffer(BufferLiteral::F32(_)) => NumericDomain::InexactFloat,
        Ir::Quote(Quoted::List(items))
            if items.iter().all(|item| matches!(item, Quoted::Int(_))) =>
        {
            NumericDomain::Exact
        }
        _ => NumericDomain::Unknown,
    }
}

/// Backend or representation passes may refine facts only after proving them.
/// This is the sole M0 path to GPU eligibility.
pub fn refine_representation(
    analysis: &mut ComputeAnalysis,
    storage: StorageClass,
    numeric_domain: NumericDomain,
) {
    analysis.storage = storage;
    analysis.numeric_domain = numeric_domain;
    analysis.gpu_blockers.retain(|blocker| {
        !matches!(
            blocker,
            AdmissionBlocker::StorageNotContiguous
                | AdmissionBlocker::NumericDomainNotRepresentable
                | AdmissionBlocker::IntegerOverflowNotProven
                | AdmissionBlocker::FloatRoundingNotDefined
        )
    });
    if storage != StorageClass::ContiguousBuffer {
        analysis
            .gpu_blockers
            .push(AdmissionBlocker::StorageNotContiguous);
    }
    if !matches!(
        numeric_domain,
        NumericDomain::FixedWidthInteger | NumericDomain::InexactFloat
    ) {
        analysis
            .gpu_blockers
            .push(AdmissionBlocker::NumericDomainNotRepresentable);
    }
    match numeric_domain {
        NumericDomain::FixedWidthInteger
            if analysis
                .region
                .as_ref()
                .is_none_or(|region| !i32_range_proven(region)) =>
        {
            analysis
                .gpu_blockers
                .push(AdmissionBlocker::IntegerOverflowNotProven);
        }
        NumericDomain::InexactFloat
            if analysis
                .region
                .as_ref()
                .is_none_or(|region| !f32_rounding_proven(region)) =>
        {
            analysis
                .gpu_blockers
                .push(AdmissionBlocker::FloatRoundingNotDefined);
        }
        _ => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComputeExecutionError {
    NotEligible(Vec<AdmissionBlocker>),
    UnsupportedOperation,
    InternalInvariant,
}

/// Common execution boundary for specialized compute backends. M0's CPU
/// implementation is the reference for the same already-admitted Kernel IR a
/// later GPU backend will consume.
pub trait ComputeBackend {
    fn execute(&self, ir: &Ir) -> Result<BufferLiteral, ComputeExecutionError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CpuComputeBackend;

impl ComputeBackend for CpuComputeBackend {
    fn execute(&self, ir: &Ir) -> Result<BufferLiteral, ComputeExecutionError> {
        let analysis = analyze(ir);
        if !analysis.gpu_eligible() {
            return Err(ComputeExecutionError::NotEligible(analysis.gpu_blockers));
        }
        let region = analysis
            .region
            .ok_or(ComputeExecutionError::UnsupportedOperation)?;
        let kernel = region
            .kernel
            .ok_or(ComputeExecutionError::InternalInvariant)?;

        match region.operation {
            BulkOperation::Map => match region.input {
                Ir::Buffer(BufferLiteral::I32(input)) => {
                    let mut output = Vec::with_capacity(input.len());
                    for element in input {
                        let value = eval_i32_range(&kernel.body, &[i64::from(element)])
                            .ok_or(ComputeExecutionError::InternalInvariant)?;
                        output.push(
                            i32::try_from(value)
                                .map_err(|_| ComputeExecutionError::InternalInvariant)?,
                        );
                    }
                    Ok(BufferLiteral::I32(output))
                }
                Ir::Buffer(BufferLiteral::F32(input)) => {
                    let offset = f32_affine_offset(&kernel.body)
                        .ok_or(ComputeExecutionError::InternalInvariant)?
                        as f32;
                    Ok(BufferLiteral::F32(
                        input
                            .into_iter()
                            .map(|bits| (f32::from_bits(bits) + offset).to_bits())
                            .collect(),
                    ))
                }
                _ => Err(ComputeExecutionError::UnsupportedOperation),
            },
            BulkOperation::Reduce => {
                let initial = region
                    .initial
                    .as_ref()
                    .ok_or(ComputeExecutionError::UnsupportedOperation)?;
                match (&region.input, initial) {
                    (Ir::Buffer(BufferLiteral::I32(input)), Ir::Int(initial_val)) => {
                        let mut acc = *initial_val;
                        let _ = i32::try_from(acc)
                            .map_err(|_| ComputeExecutionError::InternalInvariant)?;
                        for &element in input {
                            acc = eval_i32_range(&kernel.body, &[acc, i64::from(element)])
                                .ok_or(ComputeExecutionError::InternalInvariant)?;
                        }
                        let result = i32::try_from(acc)
                            .map_err(|_| ComputeExecutionError::InternalInvariant)?;
                        Ok(BufferLiteral::I32(vec![result]))
                    }
                    _ => Err(ComputeExecutionError::UnsupportedOperation),
                }
            }
        }
    }
}

/// Diagnostic report detailing worker thread usage and output buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParallelExecutionReport {
    pub output: BufferLiteral,
    pub workers_configured: usize,
    pub workers_used: usize,
    pub unique_threads: usize,
}

/// Bounded multicore CPU compute backend for pure element-wise contiguous buffers.
///
/// Divides admitted contiguous buffers into balanced deterministic partitions, executes
/// the scalar kernel across bounded OS worker threads without touching the Lisp heap,
/// and reassembles chunks in canonical input order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParallelCpuComputeBackend {
    workers: usize,
}

impl Default for ParallelCpuComputeBackend {
    fn default() -> Self {
        Self {
            workers: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4),
        }
    }
}

impl ParallelCpuComputeBackend {
    pub fn new(workers: usize) -> Self {
        Self {
            workers: workers.max(1),
        }
    }

    pub fn workers(&self) -> usize {
        self.workers
    }

    /// Deterministically partition a contiguous buffer of `len` elements across `workers`.
    ///
    /// Balances chunks such that the first `len % k` workers receive `base + 1` elements
    /// and the rest receive `base` elements.
    pub fn partition_ranges(len: usize, workers: usize) -> Vec<(usize, usize)> {
        if len == 0 || workers == 0 {
            return Vec::new();
        }
        let k = workers.min(len);
        let base = len / k;
        let rem = len % k;
        let mut ranges = Vec::with_capacity(k);
        let mut start = 0;
        for i in 0..k {
            let size = base + if i < rem { 1 } else { 0 };
            let end = start + size;
            ranges.push((start, end));
            start = end;
        }
        ranges
    }

    /// Execute the element-wise operation and report diagnostic thread usage metrics.
    pub fn execute_diagnostic(
        &self,
        ir: &Ir,
    ) -> Result<ParallelExecutionReport, ComputeExecutionError> {
        let analysis = analyze(ir);
        if !analysis.gpu_eligible() {
            return Err(ComputeExecutionError::NotEligible(analysis.gpu_blockers));
        }
        let region = analysis
            .region
            .ok_or(ComputeExecutionError::UnsupportedOperation)?;
        let kernel = region
            .kernel
            .ok_or(ComputeExecutionError::InternalInvariant)?;

        match region.operation {
            BulkOperation::Map => match region.input {
                Ir::Buffer(BufferLiteral::I32(input)) => {
                    if input.is_empty() {
                        return Ok(ParallelExecutionReport {
                            output: BufferLiteral::I32(Vec::new()),
                            workers_configured: self.workers,
                            workers_used: 0,
                            unique_threads: 0,
                        });
                    }
                    if self.workers <= 1 || input.len() == 1 {
                        let mut output = Vec::with_capacity(input.len());
                        for element in input {
                            let value = eval_i32_range(&kernel.body, &[i64::from(element)])
                                .ok_or(ComputeExecutionError::InternalInvariant)?;
                            output.push(
                                i32::try_from(value)
                                    .map_err(|_| ComputeExecutionError::InternalInvariant)?,
                            );
                        }
                        return Ok(ParallelExecutionReport {
                            output: BufferLiteral::I32(output),
                            workers_configured: self.workers,
                            workers_used: 1,
                            unique_threads: 1,
                        });
                    }

                    let ranges = Self::partition_ranges(input.len(), self.workers);
                    let workers_used = ranges.len();
                    let kernel_body = &kernel.body;
                    let input_slice = input.as_slice();

                    let chunk_results: Vec<
                        Result<(Vec<i32>, std::thread::ThreadId), ComputeExecutionError>,
                    > = std::thread::scope(|s| {
                        let mut handles = Vec::with_capacity(ranges.len());
                        for (start, end) in ranges {
                            let chunk = &input_slice[start..end];
                            let handle = s.spawn(
                            move || -> Result<(Vec<i32>, std::thread::ThreadId), ComputeExecutionError> {
                                let mut out = Vec::with_capacity(chunk.len());
                                for &element in chunk {
                                    let value = eval_i32_range(kernel_body, &[i64::from(element)])
                                        .ok_or(ComputeExecutionError::InternalInvariant)?;
                                    out.push(
                                        i32::try_from(value)
                                            .map_err(|_| ComputeExecutionError::InternalInvariant)?,
                                    );
                                }
                                Ok((out, std::thread::current().id()))
                            },
                        );
                            handles.push(handle);
                        }
                        handles
                            .into_iter()
                            .map(|h| h.join().expect("parallel cpu worker thread panicked"))
                            .collect()
                    });

                    let mut thread_ids = std::collections::HashSet::new();
                    let mut output = Vec::with_capacity(input.len());
                    for chunk_res in chunk_results {
                        let (chunk_out, tid) = chunk_res?;
                        thread_ids.insert(tid);
                        output.extend(chunk_out);
                    }

                    Ok(ParallelExecutionReport {
                        output: BufferLiteral::I32(output),
                        workers_configured: self.workers,
                        workers_used,
                        unique_threads: thread_ids.len(),
                    })
                }
                Ir::Buffer(BufferLiteral::F32(input)) => {
                    if input.is_empty() {
                        return Ok(ParallelExecutionReport {
                            output: BufferLiteral::F32(Vec::new()),
                            workers_configured: self.workers,
                            workers_used: 0,
                            unique_threads: 0,
                        });
                    }
                    let offset = f32_affine_offset(&kernel.body)
                        .ok_or(ComputeExecutionError::InternalInvariant)?
                        as f32;

                    if self.workers <= 1 || input.len() == 1 {
                        return Ok(ParallelExecutionReport {
                            output: BufferLiteral::F32(
                                input
                                    .into_iter()
                                    .map(|bits| (f32::from_bits(bits) + offset).to_bits())
                                    .collect(),
                            ),
                            workers_configured: self.workers,
                            workers_used: 1,
                            unique_threads: 1,
                        });
                    }

                    let ranges = Self::partition_ranges(input.len(), self.workers);
                    let workers_used = ranges.len();
                    let input_slice = input.as_slice();

                    let chunk_results: Vec<(Vec<u32>, std::thread::ThreadId)> =
                        std::thread::scope(|s| {
                            let mut handles = Vec::with_capacity(ranges.len());
                            for (start, end) in ranges {
                                let chunk = &input_slice[start..end];
                                let handle =
                                    s.spawn(move || -> (Vec<u32>, std::thread::ThreadId) {
                                        let out: Vec<u32> = chunk
                                            .iter()
                                            .map(|&bits| (f32::from_bits(bits) + offset).to_bits())
                                            .collect();
                                        (out, std::thread::current().id())
                                    });
                                handles.push(handle);
                            }
                            handles
                                .into_iter()
                                .map(|h| h.join().expect("parallel cpu worker thread panicked"))
                                .collect()
                        });

                    let mut thread_ids = std::collections::HashSet::new();
                    let mut output = Vec::with_capacity(input.len());
                    for (chunk_out, tid) in chunk_results {
                        thread_ids.insert(tid);
                        output.extend(chunk_out);
                    }

                    Ok(ParallelExecutionReport {
                        output: BufferLiteral::F32(output),
                        workers_configured: self.workers,
                        workers_used,
                        unique_threads: thread_ids.len(),
                    })
                }
                _ => Err(ComputeExecutionError::UnsupportedOperation),
            },
            BulkOperation::Reduce => {
                let initial = region
                    .initial
                    .as_ref()
                    .ok_or(ComputeExecutionError::UnsupportedOperation)?;
                let (Ir::Buffer(BufferLiteral::I32(input)), Ir::Int(initial_val)) =
                    (&region.input, initial)
                else {
                    return Err(ComputeExecutionError::UnsupportedOperation);
                };

                let is_parallel = analysis
                    .reduction_proof
                    .as_ref()
                    .is_some_and(|p| p.is_parallel_eligible())
                    && self.workers > 1
                    && input.len() > 1;

                if !is_parallel {
                    // Explicit fallback to sequential reference fold!
                    let mut acc = *initial_val;
                    let _ =
                        i32::try_from(acc).map_err(|_| ComputeExecutionError::InternalInvariant)?;
                    for &element in input {
                        acc = eval_i32_range(&kernel.body, &[acc, i64::from(element)])
                            .ok_or(ComputeExecutionError::InternalInvariant)?;
                    }
                    let result =
                        i32::try_from(acc).map_err(|_| ComputeExecutionError::InternalInvariant)?;
                    return Ok(ParallelExecutionReport {
                        output: BufferLiteral::I32(vec![result]),
                        workers_configured: self.workers,
                        workers_used: if input.is_empty() { 0 } else { 1 },
                        unique_threads: if input.is_empty() { 0 } else { 1 },
                    });
                }

                let ranges = Self::partition_ranges(input.len(), self.workers);
                let workers_used = ranges.len();
                let kernel_body = &kernel.body;
                let input_slice = input.as_slice();

                let chunk_results: Vec<
                    Result<(i32, std::thread::ThreadId), ComputeExecutionError>,
                > = std::thread::scope(|s| {
                    let mut handles = Vec::with_capacity(ranges.len());
                    for (idx, (start, end)) in ranges.into_iter().enumerate() {
                        let chunk = &input_slice[start..end];
                        let handle = s.spawn(
                            move || -> Result<(i32, std::thread::ThreadId), ComputeExecutionError> {
                                let init_for_worker = if idx == 0 { *initial_val } else { 0 };
                                let mut acc = init_for_worker;
                                for &element in chunk {
                                    acc = eval_i32_range(kernel_body, &[acc, i64::from(element)])
                                        .ok_or(ComputeExecutionError::InternalInvariant)?;
                                }
                                let res = i32::try_from(acc)
                                    .map_err(|_| ComputeExecutionError::InternalInvariant)?;
                                Ok((res, std::thread::current().id()))
                            },
                        );
                        handles.push(handle);
                    }
                    handles
                        .into_iter()
                        .map(|h| h.join().expect("parallel reduction worker thread panicked"))
                        .collect()
                });

                let mut thread_ids = std::collections::HashSet::new();
                let mut total_acc: i64 = 0;
                for (idx, chunk_res) in chunk_results.into_iter().enumerate() {
                    let (chunk_sum, tid) = chunk_res?;
                    thread_ids.insert(tid);
                    if idx == 0 {
                        total_acc = chunk_sum as i64;
                    } else {
                        total_acc = eval_i32_range(&kernel.body, &[total_acc, chunk_sum as i64])
                            .ok_or(ComputeExecutionError::InternalInvariant)?;
                    }
                }
                let final_result = i32::try_from(total_acc)
                    .map_err(|_| ComputeExecutionError::InternalInvariant)?;

                Ok(ParallelExecutionReport {
                    output: BufferLiteral::I32(vec![final_result]),
                    workers_configured: self.workers,
                    workers_used,
                    unique_threads: thread_ids.len(),
                })
            }
        }
    }
}

impl ComputeBackend for ParallelCpuComputeBackend {
    fn execute(&self, ir: &Ir) -> Result<BufferLiteral, ComputeExecutionError> {
        self.execute_diagnostic(ir).map(|report| report.output)
    }
}
