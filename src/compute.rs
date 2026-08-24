//! Fail-closed heterogeneous-compute analysis.
//!
//! This is deliberately an analysis layer, not a GPU backend. It recognizes
//! bulk computation in semantic IR, records the representation facts a
//! backend would need, and refuses GPU admission while any fact is unknown.

use crate::ir::{BufferLiteral, Ir, PrimOp, Quoted};

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
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputeRegion {
    pub operation: BulkOperation,
    pub function: Ir,
    pub input: Ir,
    pub initial: Option<Ir>,
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

    ComputeAnalysis { shape, effect, storage, numeric_domain, region, gpu_blockers }
}

fn extract_region(ir: &Ir) -> Option<ComputeRegion> {
    let Ir::App { func, args } = ir else { return None };
    let Ir::Var(name) = &**func else { return None };
    match (name.as_str(), args.as_slice()) {
        ("MAP", [function, input]) => Some(ComputeRegion {
            operation: BulkOperation::Map,
            function: function.clone(),
            input: input.clone(),
            initial: None,
        }),
        ("REDUCE", [function, initial, input]) => Some(ComputeRegion {
            operation: BulkOperation::Reduce,
            function: function.clone(),
            input: input.clone(),
            initial: Some(initial.clone()),
        }),
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
            let known_pure_bulk = matches!(&**func, Ir::Var(name) if name == "MAP" || name == "REDUCE");
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
        )
    });
    if storage != StorageClass::ContiguousBuffer {
        analysis.gpu_blockers.push(AdmissionBlocker::StorageNotContiguous);
    }
    if !matches!(numeric_domain, NumericDomain::FixedWidthInteger | NumericDomain::InexactFloat) {
        analysis.gpu_blockers.push(AdmissionBlocker::NumericDomainNotRepresentable);
    }
}
