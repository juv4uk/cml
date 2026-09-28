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
    /// Stored IEEE-754 binary32 bits (the same convention as
    /// `BufferLiteral::F32`): a float constant enters a kernel as bits,
    /// never as a host-decimal approximation. GPU-2-E1 / #368.
    Float32(u32),
    CheckedAdd(Box<ScalarExpr>, Box<ScalarExpr>),
    /// Language multiplication (`*`). Admission is domain-scoped: the
    /// integer fast paths reject it (no overflow proof), the binary32 path
    /// admits it only inside the scale-affine shapes (`F32MapKernel`).
    /// GPU-2-E1 / #368.
    Mul(Box<ScalarExpr>, Box<ScalarExpr>),
    /// Language subtraction (`-`). Outside closed-constant expressions this
    /// is a named refusal (GPU-2 E2 / #379).
    Sub(Box<ScalarExpr>, Box<ScalarExpr>),
    /// Language division (`/`), admitted as `x / F` or inside closed
    /// constant expressions (GPU-2 E2 / #379).
    Div(Box<ScalarExpr>, Box<ScalarExpr>),
    /// Language square root, admitted as `sqrt(x)` (GPU-2 E2 / #379).
    Sqrt(Box<ScalarExpr>),
}

/// Mechanism-side proof summary for an admitted i32 buffer.
///
/// This is not a language value. It exists so an accelerator backend can carry
/// enough evidence across a device-resident map chain to re-prove checked-add
/// kernels without copying every element back to the host.
#[cfg(any(feature = "gpu-cuda", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct I32Range {
    pub min: i32,
    pub max: i32,
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
    match (&**func, args.as_slice()) {
        (Ir::Sid(sid), [function, input])
            if *sid == sens::sid!(00110111) || *sid == sens::sid!(01011001) =>
        {
            Some(ComputeRegion {
                operation: BulkOperation::Map,
                function: function.clone(),
                input: input.clone(),
                initial: None,
                kernel: lower_kernel(function, 1),
            })
        }
        (Ir::Sid(sid), [function, initial, input]) if *sid == sens::sid!(00111001) => {
            Some(ComputeRegion {
                operation: BulkOperation::Reduce,
                function: function.clone(),
                input: input.clone(),
                initial: Some(initial.clone()),
                kernel: lower_kernel(function, 2),
            })
        }
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
        // #368/#379: float constants, multiplication, subtraction,
        // division and square root stay outside the proven-integer
        // subset; each integer operation is a separate admission
        // decision, not a copy of the float path.
        ScalarExpr::Float32(_)
        | ScalarExpr::Mul(..)
        | ScalarExpr::Sub(..)
        | ScalarExpr::Div(..)
        | ScalarExpr::Sqrt(_) => None,
    }?;
    i32::try_from(value).ok().map(i64::from)
}

#[cfg(feature = "gpu-cuda")]
pub(crate) fn i32_buffer_range(buffer: &BufferLiteral) -> Option<I32Range> {
    let BufferLiteral::I32(values) = buffer else {
        return None;
    };
    let (&first, rest) = values.split_first()?;
    let (min, max) = rest
        .iter()
        .copied()
        .fold((first, first), |(min, max), value| {
            (min.min(value), max.max(value))
        });
    Some(I32Range { min, max })
}

/// Prove and propagate the value range for one i32 map kernel.
///
/// The admitted scalar subset is monotone in each parameter: it contains only
/// parameters, exact constants, and checked addition. Therefore evaluating the
/// current input extrema gives exact extrema for every value in the interval.
/// Returning None means the next map must not execute through the resident fast
/// path.
#[cfg(any(feature = "gpu-cuda", test))]
pub(crate) fn prove_i32_map_range(function: &Ir, input: I32Range) -> Option<I32Range> {
    let kernel = lower_kernel(function, 1)?;
    let min = eval_i32_range(&kernel.body, &[i64::from(input.min)])?;
    let max = eval_i32_range(&kernel.body, &[i64::from(input.max)])?;
    Some(I32Range {
        min: i32::try_from(min).ok()?,
        max: i32::try_from(max).ok()?,
    })
}

fn substitute_parameter_zero(
    expression: &ScalarExpr,
    replacement: &ScalarExpr,
) -> Option<ScalarExpr> {
    match expression {
        ScalarExpr::Parameter(0) => Some(replacement.clone()),
        ScalarExpr::Parameter(_) => None,
        ScalarExpr::ExactInteger(value) => Some(ScalarExpr::ExactInteger(*value)),
        ScalarExpr::CheckedAdd(left, right) => Some(ScalarExpr::CheckedAdd(
            Box::new(substitute_parameter_zero(left, replacement)?),
            Box::new(substitute_parameter_zero(right, replacement)?),
        )),
        ScalarExpr::Float32(bits) => Some(ScalarExpr::Float32(*bits)),
        ScalarExpr::Mul(left, right) => Some(ScalarExpr::Mul(
            Box::new(substitute_parameter_zero(left, replacement)?),
            Box::new(substitute_parameter_zero(right, replacement)?),
        )),
        ScalarExpr::Sub(left, right) => Some(ScalarExpr::Sub(
            Box::new(substitute_parameter_zero(left, replacement)?),
            Box::new(substitute_parameter_zero(right, replacement)?),
        )),
        ScalarExpr::Div(left, right) => Some(ScalarExpr::Div(
            Box::new(substitute_parameter_zero(left, replacement)?),
            Box::new(substitute_parameter_zero(right, replacement)?),
        )),
        ScalarExpr::Sqrt(inner) => Some(ScalarExpr::Sqrt(Box::new(
            substitute_parameter_zero(inner, replacement)?,
        ))),
    }
}

/// Compose a linear unary i32 map chain without changing its evaluation
/// grouping. Every step is range-proven before its body is substituted, so
/// fusion cannot bypass the same checked-add admission used by resident chains.
#[cfg(feature = "gpu-cuda")]
pub(crate) fn fuse_i32_map_chain(
    functions: &[Ir],
    input: I32Range,
) -> Option<(ComputeKernel, I32Range)> {
    if functions.is_empty() {
        return None;
    }

    let mut body = ScalarExpr::Parameter(0);
    let mut range = input;
    for function in functions {
        let next_range = prove_i32_map_range(function, range)?;
        let kernel = lower_kernel(function, 1)?;
        body = substitute_parameter_zero(&kernel.body, &body)?;
        range = next_range;
    }

    Some((
        ComputeKernel {
            parameter_count: 1,
            body,
        },
        range,
    ))
}

fn f32_rounding_proven(region: &ComputeRegion) -> bool {
    let (Some(kernel), Ir::Buffer(BufferLiteral::F32(input))) = (&region.kernel, &region.input)
    else {
        return false;
    };
    let Some(form) = F32MapKernel::lower(&kernel.body) else {
        return false;
    };
    input.iter().all(|bits| {
        let element = f32::from_bits(*bits);
        // Canonical evaluator, per form (GPU-2-E1 / #368):
        // * the historical additive-integer form keeps its proven
        //   single-rounding parity (`eval_f64` + one narrowing);
        // * step-wise forms (float constants, multiplication) use the
        //   step-wise binary32 evaluator: A1 (sens#1585) requires every
        //   operation to round, and a single narrowing would implement
        //   FFMA semantics instead.
        let canonical = if matches!(form, F32MapKernel::AffineAdd(_)) {
            eval_f64(&kernel.body, &[f64::from(element)]).map(|value| value as f32)
        } else {
            eval_f32_stepwise(&kernel.body, &[element])
        };
        let Some(canonical) = canonical else {
            return false;
        };
        let backend = form.apply(element);
        match form {
            // Історична адитивна форма: стара перевірка без змін (#368).
            F32MapKernel::AffineAdd(_) => {
                canonical.is_finite()
                    && backend.is_finite()
                    && canonical.to_bits() == backend.to_bits()
            }
            // Нові форми (#368/#379) структурно відтворюють канонічний
            // обчислювач (та сама послідовність операцій), тому рівність
            // бітів тримає і для NaN/Inf: NaN є значенням, а не блокером.
            // Теги NaN між виконавцями порівнює свідок (sens#1585 E2),
            // не admission.
            _ => canonical.to_bits() == backend.to_bits(),
        }
    })
}

fn eval_f64(expression: &ScalarExpr, parameters: &[f64]) -> Option<f64> {
    match expression {
        ScalarExpr::Parameter(index) => parameters.get(*index).copied(),
        ScalarExpr::ExactInteger(value) => Some(*value as f64),
        ScalarExpr::Float32(bits) => Some(f64::from(f32::from_bits(*bits))),
        ScalarExpr::CheckedAdd(left, right) => {
            Some(eval_f64(left, parameters)? + eval_f64(right, parameters)?)
        }
        ScalarExpr::Mul(left, right) => {
            Some(eval_f64(left, parameters)? * eval_f64(right, parameters)?)
        }
        ScalarExpr::Sub(left, right) => {
            Some(eval_f64(left, parameters)? - eval_f64(right, parameters)?)
        }
        ScalarExpr::Div(left, right) => {
            Some(eval_f64(left, parameters)? / eval_f64(right, parameters)?)
        }
        ScalarExpr::Sqrt(inner) => eval_f64(inner, parameters).map(f64::sqrt),
    }
}

/// Step-wise binary32 evaluator: every operation rounds to f32 before the
/// next one runs. This is the canonical float evaluator for the inexact
/// domain under the ratified A1 (sens#1585): `a*b+c` must round twice, not
/// once. `eval_f64` plus a single narrowing would silently implement FFMA
/// semantics and diverge from the contract by up to the 286-ULP E1 case
/// (see the tests at the bottom of this file). GPU-2-E1 / #368.
fn eval_f32_stepwise(expression: &ScalarExpr, parameters: &[f32]) -> Option<f32> {
    match expression {
        ScalarExpr::Parameter(index) => parameters.get(*index).copied(),
        ScalarExpr::ExactInteger(value) => Some(*value as f32),
        ScalarExpr::Float32(bits) => Some(f32::from_bits(*bits)),
        ScalarExpr::CheckedAdd(left, right) => {
            Some(eval_f32_stepwise(left, parameters)? + eval_f32_stepwise(right, parameters)?)
        }
        ScalarExpr::Mul(left, right) => {
            Some(eval_f32_stepwise(left, parameters)? * eval_f32_stepwise(right, parameters)?)
        }
        ScalarExpr::Sub(left, right) => {
            Some(eval_f32_stepwise(left, parameters)? - eval_f32_stepwise(right, parameters)?)
        }
        ScalarExpr::Div(left, right) => {
            Some(eval_f32_stepwise(left, parameters)? / eval_f32_stepwise(right, parameters)?)
        }
        ScalarExpr::Sqrt(inner) => eval_f32_stepwise(inner, parameters).map(f32::sqrt),
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
            // #368/#379: float constants and the non-additive operations
            // leave the proven affine-integer shape; see `F32MapKernel`.
            ScalarExpr::Float32(_)
            | ScalarExpr::Mul(..)
            | ScalarExpr::Sub(..)
            | ScalarExpr::Div(..)
            | ScalarExpr::Sqrt(_) => None,
        }
    }
    let (parameters, constant) = collect(expression)?;
    (parameters == 1).then_some(constant)
}

/// The admitted binary32 form of one numeric-buffer-map kernel
/// (GPU-2-E1 / #368). Every variant reproduces EXACTLY the operations
/// present in the kernel body -- no phantom `+ 0.0` or `* 1.0`: the sign of
/// zero and NaN payloads depend on each single operation, so a degenerate
/// `(B, C)` pair would silently flip bits (`x * 0.0` must stay `-0.0` for
/// negative x; an implicit `+ 0.0` would return `+0.0` instead -- the E3
/// case of sens#1585). Anything outside these forms is a named refusal.
/// CML-GPU-CUDA-INT-FLOAT-ADMISSION-DOCS.
pub(crate) enum F32MapKernel {
    /// Historical additive-integer form `x + C` (flattened tree). Its
    /// rounding parity is the proven `eval_f64` + single narrowing.
    AffineAdd(f32),
    /// `x + F` with one Float32 constant: a single step-wise operation.
    Add(f32),
    /// `x * F`: a single step-wise operation (E3: `-1.0 * 0.0` stays -0.0).
    Mul(f32),
    /// `x * F + G`: exactly two step-wise roundings. A1 (sens#1585):
    /// FFMA contraction is forbidden in the witness slice; the emitters
    /// emit the two plain operators, and contraction is controlled by the
    /// kernel mode (`-fmad=false` in `CudaKernelMode::BitwiseEquality`,
    /// PR #366), not by the emitter -- the production-mode FMA policy is a
    /// separate language-owner decision.
    MulAdd(f32, f32),
    /// `x / F`: one step-wise operation (E2: `0.0 / 0.0` is a NaN value,
    /// not a blocker -- tags are compared by the witness, sens#1585).
    Div(f32),
    /// `sqrt(x)`: one step-wise, IEEE-correctly-rounded operation
    /// (E2: `sqrt(-1.0)` is a NaN value).
    Sqrt,
    /// A closed constant expression (no `Parameter` in the tree): the
    /// kernel broadcasts the step-wise-computed stored bits. The emitters
    /// emit those very bits, so canonical == backend bitwise by
    /// construction, even for NaN/Inf (E2: `inf - inf`). GPU-2 / #379.
    Constant(u32),
}

impl F32MapKernel {
    /// Classify an admitted kernel body. None: the region is not admitted
    /// to the float fast path.
    pub(crate) fn lower(body: &ScalarExpr) -> Option<F32MapKernel> {
        fn parameter_zero(expression: &ScalarExpr) -> bool {
            matches!(expression, ScalarExpr::Parameter(0))
        }
        fn constant(expression: &ScalarExpr) -> Option<f32> {
            match expression {
                ScalarExpr::ExactInteger(value) => Some(*value as f32),
                ScalarExpr::Float32(bits) => Some(f32::from_bits(*bits)),
                _ => None,
            }
        }
        fn mul(expression: &ScalarExpr) -> Option<f32> {
            match expression {
                ScalarExpr::Mul(left, right) if parameter_zero(left) => constant(right),
                ScalarExpr::Mul(left, right) if parameter_zero(right) => constant(left),
                _ => None,
            }
        }
        if let Some(offset) = f32_affine_offset(body) {
            return Some(F32MapKernel::AffineAdd(offset as f32));
        }
        match body {
            ScalarExpr::CheckedAdd(left, right) => {
                if parameter_zero(left) {
                    return constant(right).map(F32MapKernel::Add);
                }
                if parameter_zero(right) {
                    return constant(left).map(F32MapKernel::Add);
                }
                if let (Some(scale), Some(offset)) = (mul(left), constant(right)) {
                    return Some(F32MapKernel::MulAdd(scale, offset));
                }
                if let (Some(scale), Some(offset)) = (mul(right), constant(left)) {
                    return Some(F32MapKernel::MulAdd(scale, offset));
                }
                None
            }
            ScalarExpr::Mul(..) => mul(body).map(F32MapKernel::Mul),
            ScalarExpr::Div(left, right) if parameter_zero(left) => {
                constant(right).map(F32MapKernel::Div)
            }
            ScalarExpr::Sqrt(inner) if parameter_zero(inner) => Some(F32MapKernel::Sqrt),
            _ => None,
        }
        .or_else(|| closed_constant_bits(body).map(F32MapKernel::Constant))
    }

    /// Apply the form with exactly its own operations.
    pub(crate) fn apply(&self, element: f32) -> f32 {
        match self {
            F32MapKernel::AffineAdd(offset) | F32MapKernel::Add(offset) => element + offset,
            F32MapKernel::Mul(scale) => element * scale,
            F32MapKernel::MulAdd(scale, offset) => element * scale + offset,
            F32MapKernel::Div(divisor) => element / divisor,
            F32MapKernel::Sqrt => element.sqrt(),
            F32MapKernel::Constant(bits) => f32::from_bits(*bits),
        }
    }
}

/// Evaluate a closed (no `Parameter`) float expression step-wise and return
/// its stored bits. None: the tree references a parameter or holds a node
/// outside the float subset. GPU-2 E2/E3 / #379.
fn closed_constant_bits(expression: &ScalarExpr) -> Option<u32> {
    fn closed(expression: &ScalarExpr) -> bool {
        match expression {
            ScalarExpr::Parameter(_) => false,
            ScalarExpr::ExactInteger(_) | ScalarExpr::Float32(_) => true,
            ScalarExpr::CheckedAdd(left, right)
            | ScalarExpr::Mul(left, right)
            | ScalarExpr::Sub(left, right)
            | ScalarExpr::Div(left, right) => closed(left) && closed(right),
            ScalarExpr::Sqrt(inner) => closed(inner),
        }
    }
    if !closed(expression) {
        return None;
    }
    eval_f32_stepwise(expression, &[]).map(|value| value.to_bits())
}

fn lower_kernel(function: &Ir, expected_parameters: usize) -> Option<ComputeKernel> {
    if expected_parameters == 2 {
        let is_add = matches!(
            function,
            Ir::Sid(sid) if *sid == sens::sid!(00001100)
        );
        if is_add {
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

/// The Sens8 identity of a callable surface, resolved through the same
/// semantic registry the lowering itself uses -- no invented SIDs. None
/// means the surface is not an admitted callable, and the operation simply
/// does not lower into the compute region. GPU-2 / #368 / #379.
fn surface_sid(surface: &str) -> Option<sens::Sid8> {
    crate::canon::callable_semantic_id(surface)
}

fn lower_scalar_expr(ir: &Ir, parameters: &[String]) -> Option<ScalarExpr> {
    match ir {
        Ir::Int(value) => Some(ScalarExpr::ExactInteger(*value)),
        // f64 -> binary32 bits: the constant enters the kernel as stored
        // bits, never a host-decimal approximation (GPU-2-E1 / #368).
        Ir::Float(value) => Some(ScalarExpr::Float32((*value as f32).to_bits())),
        Ir::Var(name) => parameters
            .iter()
            .position(|parameter| parameter == name)
            .map(ScalarExpr::Parameter),
        Ir::App { func, args }
            if matches!(&**func, Ir::Sid(sid) if *sid == sens::sid!(00001100))
                && args.len() == 2 =>
        {
            Some(ScalarExpr::CheckedAdd(
                Box::new(lower_scalar_expr(&args[0], parameters)?),
                Box::new(lower_scalar_expr(&args[1], parameters)?),
            ))
        }
        // Language multiplication: identity comes from the registry (see
        // `surface_sid`); admission is domain-scoped and lives in
        // `F32MapKernel` / the integer paths. GPU-2-E1 / #368.
        Ir::App { func, args }
            if args.len() == 2
                && matches!(&**func, Ir::Sid(sid)
                    if surface_sid("*").map_or(false, |mul| mul == *sid)) =>
        {
            Some(ScalarExpr::Mul(
                Box::new(lower_scalar_expr(&args[0], parameters)?),
                Box::new(lower_scalar_expr(&args[1], parameters)?),
            ))
        }
        // GPU-2 E2/E3 / #379: subtraction and division lower for the float
        // domain (closed-constant forms and `x / F`); admission stays
        // domain-scoped, the integer paths refuse them by name.
        Ir::App { func, args }
            if args.len() == 2
                && matches!(&**func, Ir::Sid(sid)
                    if surface_sid("-").map_or(false, |sub| sub == *sid)) =>
        {
            Some(ScalarExpr::Sub(
                Box::new(lower_scalar_expr(&args[0], parameters)?),
                Box::new(lower_scalar_expr(&args[1], parameters)?),
            ))
        }
        Ir::App { func, args }
            if args.len() == 2
                && matches!(&**func, Ir::Sid(sid)
                    if surface_sid("/").map_or(false, |div| div == *sid)) =>
        {
            Some(ScalarExpr::Div(
                Box::new(lower_scalar_expr(&args[0], parameters)?),
                Box::new(lower_scalar_expr(&args[1], parameters)?),
            ))
        }
        // GPU-2 E2 / #379: unary square root.
        Ir::App { func, args }
            if args.len() == 1
                && matches!(&**func, Ir::Sid(sid)
                    if surface_sid("sqrt").map_or(false, |s| s == *sid)) =>
        {
            Some(ScalarExpr::Sqrt(Box::new(
                lower_scalar_expr(&args[0], parameters)?,
            )))
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
        | Ir::Builtin(_)
        // A standalone SID8 value is a pure word (first-class callable identity).
        | Ir::Sid(_) => EffectClass::Pure,
        Ir::Lambda { body, .. } => effect_of(body),
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
            if matches!(
                &**func,
                Ir::Sid(sid) if *sid == sens::sid!(00000100)
            ) {
                return EffectClass::Allocating;
            }
            let known_pure = matches!(
                &**func,
                Ir::Sid(sid)
                    if *sid == sens::sid!(00001100)
                        || *sid == sens::sid!(00110111)
                        || *sid == sens::sid!(01011001)
                        || *sid == sens::sid!(00111001)
            );
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
                    let form = F32MapKernel::lower(&kernel.body)
                        .ok_or(ComputeExecutionError::InternalInvariant)?;
                    Ok(BufferLiteral::F32(
                        input
                            .into_iter()
                            .map(|bits| form.apply(f32::from_bits(bits)).to_bits())
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
                    let form = F32MapKernel::lower(&kernel.body)
                        .ok_or(ComputeExecutionError::InternalInvariant)?;

                    if self.workers <= 1 || input.len() == 1 {
                        return Ok(ParallelExecutionReport {
                            output: BufferLiteral::F32(
                                input
                                    .into_iter()
                                    .map(|bits| form.apply(f32::from_bits(bits)).to_bits())
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
                                            .map(|&bits| form.apply(f32::from_bits(bits)).to_bits())
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

#[cfg(test)]
mod resident_range_tests {
    use super::*;

    fn add_constant(offset: i64) -> Ir {
        Ir::Lambda {
            params: Params::Fixed(vec!["x".into()]),
            body: Box::new(Ir::App {
                func: Box::new(Ir::Sid(sens::sens!(00001100))),
                args: vec![Ir::Var("x".into()), Ir::Int(offset)],
            }),
        }
    }

    #[test]
    fn i32_resident_range_propagates_without_element_materialization() {
        let input = I32Range { min: -10, max: 20 };
        assert_eq!(
            prove_i32_map_range(&add_constant(7), input),
            Some(I32Range { min: -3, max: 27 })
        );
    }

    #[test]
    fn i32_resident_range_rejects_second_step_overflow() {
        let add_one = add_constant(1);
        let first = prove_i32_map_range(
            &add_one,
            I32Range {
                min: i32::MAX - 1,
                max: i32::MAX - 1,
            },
        )
        .expect("first step remains in i32");
        assert_eq!(first.min, i32::MAX);
        assert_eq!(first.max, i32::MAX);
        assert_eq!(prove_i32_map_range(&add_one, first), None);
    }

    #[cfg(feature = "gpu-cuda")]
    #[test]
    fn fused_i32_chain_preserves_sequential_grouping() {
        let (kernel, range) = fuse_i32_map_chain(
            &[add_constant(1), add_constant(2)],
            I32Range { min: -5, max: 10 },
        )
        .expect("bounded add chain should fuse");
        assert_eq!(kernel.parameter_count, 1);
        assert_eq!(
            kernel.body,
            ScalarExpr::CheckedAdd(
                Box::new(ScalarExpr::CheckedAdd(
                    Box::new(ScalarExpr::Parameter(0)),
                    Box::new(ScalarExpr::ExactInteger(1)),
                )),
                Box::new(ScalarExpr::ExactInteger(2)),
            )
        );
        assert_eq!(range, I32Range { min: -2, max: 13 });
    }

    #[cfg(feature = "gpu-cuda")]
    #[test]
    fn fused_i32_chain_rejects_intermediate_overflow() {
        assert_eq!(
            fuse_i32_map_chain(
                &[add_constant(1), add_constant(1)],
                I32Range {
                    min: i32::MAX - 1,
                    max: i32::MAX - 1,
                },
            ),
            None
        );
    }
}

#[cfg(test)]
mod f32_map_kernel_tests {
    use super::*;

    /// Ratified §1 constants (sens#1585 E1) as stored binary32 bits.
    const E1_A: u32 = 0x3e9c03e1; // 0.3047171
    const E1_B: u32 = 0xbf851e33; // -1.0399841
    const E1_C: u32 = 0x3ea26003; // 0.31713876

    fn e1_body() -> ScalarExpr {
        ScalarExpr::CheckedAdd(
            Box::new(ScalarExpr::Mul(
                Box::new(ScalarExpr::Parameter(0)),
                Box::new(ScalarExpr::Float32(E1_B)),
            )),
            Box::new(ScalarExpr::Float32(E1_C)),
        )
    }

    #[test]
    fn map_kernel_classifies_the_e1_shape() {
        match F32MapKernel::lower(&e1_body()) {
            Some(F32MapKernel::MulAdd(scale, offset)) => {
                assert_eq!(scale.to_bits(), E1_B);
                assert_eq!(offset.to_bits(), E1_C);
            }
            other => panic!("E1 body must classify as MulAdd, got {other:?}"),
        }
    }

    #[test]
    fn affine_trees_keep_the_single_add_form() {
        let nested = ScalarExpr::CheckedAdd(
            Box::new(ScalarExpr::CheckedAdd(
                Box::new(ScalarExpr::Parameter(0)),
                Box::new(ScalarExpr::ExactInteger(1)),
            )),
            Box::new(ScalarExpr::ExactInteger(2)),
        );
        match F32MapKernel::lower(&nested) {
            Some(F32MapKernel::AffineAdd(offset)) => assert_eq!(offset, 3.0),
            other => panic!("additive tree must classify as AffineAdd, got {other:?}"),
        }
    }

    #[test]
    fn pure_multiplication_preserves_negative_zero() {
        // E3 (sens#1585): (-1.0) * (+0.0) = -0.0. A degenerate form with an
        // implicit `+ 0.0` would flip it to +0.0, so a bare multiply must
        // stay a bare multiply.
        let body = ScalarExpr::Mul(
            Box::new(ScalarExpr::Parameter(0)),
            Box::new(ScalarExpr::Float32(0x00000000)),
        );
        let form = F32MapKernel::lower(&body).expect("pure multiply is admitted");
        assert!(matches!(form, F32MapKernel::Mul(scale) if scale.to_bits() == 0));
        let element = f32::from_bits(0xbf800000); // -1.0
        let canonical =
            eval_f32_stepwise(&body, &[element]).expect("stepwise evaluates the multiply");
        assert_eq!(canonical.to_bits(), 0x80000000, "(-1.0)*(+0.0) is -0.0");
        assert_eq!(form.apply(element).to_bits(), 0x80000000);
    }

    #[test]
    fn stepwise_f32_diverges_from_single_narrowing_on_e1() {
        // The 286-ULP E1 case (sens#1585 §1): step-wise binary32 rounds
        // twice (0x39796000), while f64 with a single narrowing reproduces
        // FFMA semantics (0x3979611e). This is why the inexact-domain
        // canonical evaluator is step-wise, not narrowed-once. GPU-2-E1 /
        // #368.
        let element = f32::from_bits(E1_A);
        let stepwise =
            eval_f32_stepwise(&e1_body(), &[element]).expect("E1 body evaluates step-wise");
        let fused = eval_f64(&e1_body(), &[f64::from(element)])
            .expect("E1 body evaluates in f64") as f32;
        assert_eq!(stepwise.to_bits(), 0x39796000);
        assert_eq!(fused.to_bits(), 0x3979611e);
    }

    fn surface(name: &str) -> sens::Sid8 {
        surface_sid(name).unwrap_or_else(|| panic!("'{name}' must be an admitted surface"))
    }

    fn map_region(body: Ir, element_bits: u32) -> Ir {
        Ir::App {
            func: Box::new(Ir::Sid(sens::sens!(01011001))),
            args: vec![
                Ir::Lambda {
                    params: Params::Fixed(vec!["x".into()]),
                    body: Box::new(body),
                },
                Ir::Buffer(BufferLiteral::F32(vec![element_bits])),
            ],
        }
    }

    fn execute_bits(region: &Ir) -> Vec<u32> {
        let output = CpuComputeBackend
            .execute(region)
            .expect("region must be admitted and executed");
        let BufferLiteral::F32(bits) = output else {
            panic!("f32 output expected");
        };
        bits
    }

    #[test]
    fn e2_div_by_zero_produces_nan_on_the_cpu_backend() {
        // e2_qnan_0div0: (/ x 0.0) на #f32(0.0) — NaN є значенням (тег
        // порівнює свідок), admission не вимагає скінченності.
        let region = map_region(
            Ir::App {
                func: Box::new(Ir::Sid(surface("/"))),
                args: vec![Ir::Var("x".into()), Ir::Float(0.0)],
            },
            0x00000000,
        );
        let bits = execute_bits(&region);
        assert!(f32::from_bits(bits[0]).is_nan(), "0.0/0.0 is NaN");
    }

    #[test]
    fn e2_closed_constant_inf_minus_inf_is_nan() {
        // e2_qnan_inf_minus_inf: (- (/ 1.0 0.0) (/ 1.0 0.0)) — параметр не
        // використано; ядро — broadcast замкненої сталої.
        let region = map_region(
            Ir::App {
                func: Box::new(Ir::Sid(surface("-"))),
                args: vec![
                    Ir::App {
                        func: Box::new(Ir::Sid(surface("/"))),
                        args: vec![Ir::Float(1.0), Ir::Float(0.0)],
                    },
                    Ir::App {
                        func: Box::new(Ir::Sid(surface("/"))),
                        args: vec![Ir::Float(1.0), Ir::Float(0.0)],
                    },
                ],
            },
            0x3f800000,
        );
        let bits = execute_bits(&region);
        assert!(f32::from_bits(bits[0]).is_nan(), "inf - inf is NaN");
        let form_body = ScalarExpr::Sub(
            Box::new(ScalarExpr::Div(
                Box::new(ScalarExpr::Float32(0x3f800000)),
                Box::new(ScalarExpr::Float32(0x00000000)),
            )),
            Box::new(ScalarExpr::Div(
                Box::new(ScalarExpr::Float32(0x3f800000)),
                Box::new(ScalarExpr::Float32(0x00000000)),
            )),
        );
        match F32MapKernel::lower(&form_body) {
            Some(F32MapKernel::Constant(bits)) => {
                assert!(f32::from_bits(bits).is_nan());
            }
            other => panic!("closed constant must classify as Constant, got {other:?}"),
        }
    }

    #[test]
    fn e2_sqrt_of_negative_is_nan() {
        // e2_qnan_sqrt_neg: (sqrt x) на #f32(-1.0).
        let region = map_region(
            Ir::App {
                func: Box::new(Ir::Sid(surface("sqrt"))),
                args: vec![Ir::Var("x".into())],
            },
            0xbf800000,
        );
        let bits = execute_bits(&region);
        assert!(f32::from_bits(bits[0]).is_nan(), "sqrt(-1.0) is NaN");
        assert!(matches!(
            F32MapKernel::lower(&ScalarExpr::Sqrt(Box::new(ScalarExpr::Parameter(0)))),
            Some(F32MapKernel::Sqrt)
        ));
    }

    #[test]
    fn e3_zero_sign_cases_execute_bitwise() {
        // e3_add_pos0_neg0: (+ x -0.0) на #f32(0.0) → +0.0 (RNE).
        let region = map_region(
            Ir::App {
                func: Box::new(Ir::Sid(sens::sens!(00001100))),
                args: vec![Ir::Var("x".into()), Ir::Float(-0.0)],
            },
            0x00000000,
        );
        assert_eq!(execute_bits(&region), vec![0x00000000]);
        // e3_mul_neg1_pos0: (* x 0.0) на #f32(-1.0) → -0.0.
        let region = map_region(
            Ir::App {
                func: Box::new(Ir::Sid(surface("*"))),
                args: vec![Ir::Var("x".into()), Ir::Float(0.0)],
            },
            0xbf800000,
        );
        assert_eq!(execute_bits(&region), vec![0x80000000]);
    }

    #[test]
    fn e1_region_admits_and_executes_on_the_cpu_backend() {
        let Some(mul) = surface_sid("*") else {
            panic!("'*' must remain an admitted callable surface");
        };
        let body = Ir::App {
            func: Box::new(Ir::Sid(sens::sens!(00001100))),
            args: vec![
                Ir::App {
                    func: Box::new(Ir::Sid(mul)),
                    args: vec![Ir::Var("x".into()), Ir::Float(-1.0399841)],
                },
                Ir::Float(0.31713876),
            ],
        };
        let region = Ir::App {
            func: Box::new(Ir::Sid(sens::sens!(01011001))),
            args: vec![
                Ir::Lambda {
                    params: Params::Fixed(vec!["x".into()]),
                    body: Box::new(body),
                },
                Ir::Buffer(BufferLiteral::F32(vec![E1_A])),
            ],
        };
        let output = CpuComputeBackend
            .execute(&region)
            .expect("E1 scale-affine region must be admitted and executed");
        let BufferLiteral::F32(bits) = output else {
            panic!("E1 output must be an f32 buffer");
        };
        assert_eq!(bits.as_slice(), &[0x39796000]);
    }
}
