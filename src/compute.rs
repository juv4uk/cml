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

#[derive(Debug, Clone, PartialEq)]
pub struct ComputeAnalysis {
    pub shape: ExecutionShape,
    pub effect: EffectClass,
    pub storage: StorageClass,
    pub numeric_domain: NumericDomain,
    pub region: Option<ComputeRegion>,
    pub gpu_blockers: Vec<AdmissionBlocker>,
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
    if !matches!(shape, ExecutionShape::ElementWise | ExecutionShape::Reduction) {
        gpu_blockers.push(AdmissionBlocker::NotBulkParallel);
    }
    if effect != EffectClass::Pure {
        gpu_blockers.push(AdmissionBlocker::EffectNotPure);
    }
    if storage != StorageClass::ContiguousBuffer {
        gpu_blockers.push(AdmissionBlocker::StorageNotContiguous);
    }
    if !matches!(numeric_domain, NumericDomain::FixedWidthInteger | NumericDomain::InexactFloat) {
        gpu_blockers.push(AdmissionBlocker::NumericDomainNotRepresentable);
    }
    if region.as_ref().is_some_and(|region| region.kernel.is_none()) {
        gpu_blockers.push(AdmissionBlocker::KernelNotLowerable);
    }
    match (numeric_domain, region.as_ref()) {
        (NumericDomain::FixedWidthInteger, Some(region))
            if !i32_range_proven(region) =>
        {
            gpu_blockers.push(AdmissionBlocker::IntegerOverflowNotProven);
        }
        (NumericDomain::InexactFloat, Some(region)) if !f32_rounding_proven(region) => {
            gpu_blockers.push(AdmissionBlocker::FloatRoundingNotDefined);
        }
        _ => {}
    }

    ComputeAnalysis { shape, effect, storage, numeric_domain, region, gpu_blockers }
}

fn extract_region(ir: &Ir) -> Option<ComputeRegion> {
    let Ir::App { func, args } = ir else { return None };
    let Ir::Var(name) = &**func else { return None };
    match (name.as_str(), args.as_slice()) {
        ("MAP" | "NUMERIC-BUFFER-MAP", [function, input]) => Some(ComputeRegion {
            operation: BulkOperation::Map,
            function: function.clone(),
            input: input.clone(),
            initial: None,
            kernel: lower_kernel(function, 1),
        }),
        ("REDUCE", [function, initial, input]) => Some(ComputeRegion {
            operation: BulkOperation::Reduce,
            function: function.clone(),
            input: input.clone(),
            initial: Some(initial.clone()),
            kernel: lower_kernel(function, 2),
        }),
        _ => None,
    }
}

fn i32_range_proven(region: &ComputeRegion) -> bool {
    let (Some(kernel), Ir::Buffer(BufferLiteral::I32(input))) = (&region.kernel, &region.input)
    else {
        return false;
    };
    input.iter().all(|element| {
        eval_i32_range(&kernel.body, &[i64::from(*element)])
            .is_some_and(|value| i32::try_from(value).is_ok())
    })
}

fn eval_i32_range(expression: &ScalarExpr, parameters: &[i64]) -> Option<i64> {
    match expression {
        ScalarExpr::Parameter(index) => parameters.get(*index).copied(),
        ScalarExpr::ExactInteger(value) => Some(*value),
        ScalarExpr::CheckedAdd(left, right) => eval_i32_range(left, parameters)?
            .checked_add(eval_i32_range(right, parameters)?),
    }
}

fn f32_rounding_proven(region: &ComputeRegion) -> bool {
    let (Some(kernel), Ir::Buffer(BufferLiteral::F32(input))) = (&region.kernel, &region.input) else {
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
    let Ir::Lambda { params: Params::Fixed(parameters), body } = function else {
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
        _ => None,
    }
}

fn is_scalar(ir: &Ir) -> bool {
    matches!(ir, Ir::Int(_) | Ir::Nil | Ir::True | Ir::Var(_) | Ir::Prim { .. })
}

fn effect_of(ir: &Ir) -> EffectClass {
    match ir {
        Ir::Int(_) | Ir::Buffer(_) | Ir::Nil | Ir::True | Ir::Var(_) | Ir::Quote(_) => {
            EffectClass::Pure
        }
        Ir::Lambda { body, .. } => effect_of(body),
        Ir::Prim { op: PrimOp::Cons, .. } => EffectClass::Allocating,
        Ir::Prim { args, .. } => join_effects(args.iter().map(effect_of)),
        Ir::Cond { branches } => join_effects(
            branches.iter().flat_map(|(test, body)| [effect_of(test), effect_of(body)]),
        ),
        Ir::Let { bindings, body } => join_effects(
            bindings.iter().map(|(_, value)| effect_of(value)).chain([effect_of(body)]),
        ),
        Ir::Def { .. } => EffectClass::Stateful,
        Ir::App { func, args } => {
            let known_pure_bulk = matches!(&**func, Ir::Var(name) if name == "MAP" || name == "NUMERIC-BUFFER-MAP" || name == "REDUCE");
            if known_pure_bulk {
                join_effects(args.iter().map(effect_of))
            } else {
                EffectClass::Unknown
            }
        }
    }
}

fn join_effects(effects: impl IntoIterator<Item = EffectClass>) -> EffectClass {
    effects.into_iter().fold(EffectClass::Pure, |left, right| match (left, right) {
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
        Ir::Quote(Quoted::List(items)) if items.iter().all(|item| matches!(item, Quoted::Int(_))) => {
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
        analysis.gpu_blockers.push(AdmissionBlocker::StorageNotContiguous);
    }
    if !matches!(numeric_domain, NumericDomain::FixedWidthInteger | NumericDomain::InexactFloat) {
        analysis.gpu_blockers.push(AdmissionBlocker::NumericDomainNotRepresentable);
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
        let region = analysis.region.ok_or(ComputeExecutionError::UnsupportedOperation)?;
        if region.operation != BulkOperation::Map {
            return Err(ComputeExecutionError::UnsupportedOperation);
        }
        let kernel = region.kernel.ok_or(ComputeExecutionError::InternalInvariant)?;
        match region.input {
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
                    .ok_or(ComputeExecutionError::InternalInvariant)? as f32;
                Ok(BufferLiteral::F32(
                    input
                        .into_iter()
                        .map(|bits| (f32::from_bits(bits) + offset).to_bits())
                        .collect(),
                ))
            }
            _ => Err(ComputeExecutionError::UnsupportedOperation),
        }
    }
}
