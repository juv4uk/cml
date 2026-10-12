//! SENS-specific, proof-gated optimizer for the CURRENT exact-domain CML IR.
//!
//! NOT a Common Lisp evaluator, not a generic SBCL IR clone and not a machine
//! backend. No byte/SID identities, guessed spellings or host-specific truth
//! conventions are used to select a rewrite.
//!
//! The only first-stage rewrites are:
//!   HEAD(PAIR(QUOTE a, QUOTE b)) -> QUOTE a
//!   TAIL(PAIR(QUOTE a, QUOTE b)) -> QUOTE b
//! where all three callable roles are freshly proved by SENS's executable
//! compiler law and both PAIR operands are quoted, hence eager evaluation is
//! pure and total for the admitted exact quoted-data subset.
//!
//! We do NOT rewrite HEAD(PAIR(expr, expr)): doing so could skip the second
//! expression's evaluation, including its errors or effects. We do NOT
//! distribute over COND, reduce ATOM/EQ, guess D1 truth values or touch numeric
//! domains. Any change to this policy needs a separate SENS-owned law + tests.
//!
//! A witness is an auditable derivation, not a mechanically formal proof.
//! Independent SENS-oracle differential tests are mandatory before promotion.
//! This module is candidate-only until native CML codegen has its own parity.

use crate::current_exact_model::{
    lower, lower_canonical_t5_bytes, verified_role, CmlMechanism, ModernIr, ModernLowerError,
    VerifiedProgramIr, VerifiedRole,
};
use sens::{CompilerLoweringRole, CoreDomainIdentity};

const MAX_DEPTH: usize = 128;
const MAX_NODES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteLaw {
    QuotedHeadOfFreshPair,
    QuotedTailOfFreshPair,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteWitness {
    pub law: RewriteLaw,
    pub selector_identity: CoreDomainIdentity,
    pub pair_identity: CoreDomainIdentity,
    pub first_quote_identity: CoreDomainIdentity,
    pub second_quote_identity: CoreDomainIdentity,
    pub before_nodes: usize,
    pub after_nodes: usize,
    pub authority_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizedProgram {
    /// Immutable SHA-256 of the physical, canonical original T5 input.
    pub canonical_t5_sha256: [u8; 32],
    pub original_nodes: usize,
    pub optimized_nodes: usize,
    /// Only rewrites with strictly reduced node cost are admitted.
    pub ir: Vec<ModernIr>,
    /// Ordered proof-step evidence (SENS exact identities + authority digest).
    pub witnesses: Vec<RewriteWitness>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizeError {
    EmptyProgram,
    TooDeep,
    NodeLimitExceeded,
    UnprovedRole,
    RoleTargetMismatch,
    InvalidArity,
    /// A caller may not attach a plausible IR to a different admitted AST.
    AstIrMismatch,
}

fn require_role(binding: &VerifiedRole, expected: CompilerLoweringRole)
    -> Result<(), OptimizeError>
{
    let actual = verified_role(binding.identity).map_err(|_| OptimizeError::UnprovedRole)?;
    if &actual != binding {
        return Err(OptimizeError::UnprovedRole);
    }
    if binding.role != expected {
        return Err(OptimizeError::RoleTargetMismatch);
    }
    Ok(())
}

fn target_role(target: CmlMechanism) -> CompilerLoweringRole {
    match target {
        CmlMechanism::AtomPredicate => CompilerLoweringRole::AtomPredicate,
        CmlMechanism::Head => CompilerLoweringRole::SelectorHead,
        CmlMechanism::Tail => CompilerLoweringRole::SelectorTail,
        CmlMechanism::AtomEquality => CompilerLoweringRole::AtomEquality,
        CmlMechanism::PairConstruct => CompilerLoweringRole::PairConstruct,
    }
}

fn target_arity(target: CmlMechanism) -> usize {
    match target {
        CmlMechanism::AtomPredicate | CmlMechanism::Head | CmlMechanism::Tail => 1,
        CmlMechanism::AtomEquality | CmlMechanism::PairConstruct => 2,
    }
}

fn verify_ir_node(node: &ModernIr, depth: usize, remaining: &mut usize)
    -> Result<usize, OptimizeError>
{
    if depth >= MAX_DEPTH {
        return Err(OptimizeError::TooDeep);
    }
    if *remaining == 0 {
        return Err(OptimizeError::NodeLimitExceeded);
    }
    *remaining -= 1;
    let mut nodes = 1usize;
    match node {
        ModernIr::Literal(_) => (),
        ModernIr::Quote { binding, .. } =>
            require_role(binding, CompilerLoweringRole::QuoteForm)?,
        ModernIr::Mechanism { binding, target, args } => {
            require_role(binding, target_role(*target))?;
            if args.len() != target_arity(*target) {
                return Err(OptimizeError::InvalidArity);
            }
            for arg in args {
                nodes += verify_ir_node(arg, depth + 1, remaining)?;
            }
        }
        ModernIr::Conditional { binding, branches } => {
            require_role(binding, CompilerLoweringRole::CondForm)?;
            for (test, branch) in branches {
                nodes += verify_ir_node(test, depth + 1, remaining)?;
                nodes += verify_ir_node(branch, depth + 1, remaining)?;
            }
        }
    }
    Ok(nodes)
}

fn measure(nodes: &[ModernIr]) -> Result<usize, OptimizeError> {
    if nodes.is_empty() {
        return Err(OptimizeError::EmptyProgram);
    }
    let mut remaining = MAX_NODES;
    let mut count = 0usize;
    for node in nodes {
        count += verify_ir_node(node, 0, &mut remaining)?;
    }
    Ok(count)
}

/// Match only a freshly constructed pair whose BOTH eager operands are
/// admitted quoted constants. No general purity speculation is allowed.
fn select_pair_of_quotes(
    node: &ModernIr,
) -> Option<(ModernIr, RewriteWitness)> {
    let ModernIr::Mechanism { binding, target, args } = node else { return None };
    let (law, chosen) = match target {
        CmlMechanism::Head => (RewriteLaw::QuotedHeadOfFreshPair, 0),
        CmlMechanism::Tail => (RewriteLaw::QuotedTailOfFreshPair, 1),
        _ => return None,
    };
    let [ModernIr::Mechanism {
        binding: pair_binding, target: CmlMechanism::PairConstruct, args: pair_args
    }] = args.as_slice() else { return None };
    let [ModernIr::Quote {binding: first, ..}, ModernIr::Quote {binding: second, ..}]
        = pair_args.as_slice() else { return None };

    // The full tree has already been checked by verify_ir_node(), but require
    // the particular proof roles again at the rewrite boundary.
    require_role(binding, target_role(*target)).ok()?;
    require_role(pair_binding, CompilerLoweringRole::PairConstruct).ok()?;
    require_role(first, CompilerLoweringRole::QuoteForm).ok()?;
    require_role(second, CompilerLoweringRole::QuoteForm).ok()?;
    if binding.authority_sha256 != pair_binding.authority_sha256
        || binding.authority_sha256 != first.authority_sha256
        || binding.authority_sha256 != second.authority_sha256 {
        return None;
    }

    Some((pair_args[chosen].clone(), RewriteWitness {
        law,
        selector_identity: binding.identity,
        pair_identity: pair_binding.identity,
        first_quote_identity: first.identity,
        second_quote_identity: second.identity,
        before_nodes: 4,
        after_nodes: 1,
        authority_sha256: binding.authority_sha256.clone(),
    }))
}

fn rewrite_verified(node: &ModernIr, witnesses: &mut Vec<RewriteWitness>) -> ModernIr {
    let rebuilt = match node {
        ModernIr::Literal(_) | ModernIr::Quote { .. } => node.clone(),
        ModernIr::Mechanism { binding, target, args } => ModernIr::Mechanism {
            binding: binding.clone(),
            target: *target,
            args: args.iter().map(|a| rewrite_verified(a, witnesses)).collect(),
        },
        ModernIr::Conditional { binding, branches } => ModernIr::Conditional {
            binding: binding.clone(),
            branches: branches.iter().map(|(a, b)| {
                (rewrite_verified(a, witnesses), rewrite_verified(b, witnesses))
            }).collect(),
        },
    };
    if let Some((reduced, certificate)) = select_pair_of_quotes(&rebuilt) {
        witnesses.push(certificate);
        return reduced;
    }
    rebuilt
}

/// Candidate-only cost model: fewer IR nodes (NOT a claim of faster native
/// execution, lower energy, or smaller physical T5). Recompute and validate
/// every role through the current SENS executable law before accepting output.
pub fn optimize_exact_ir(input: &VerifiedProgramIr)
    -> Result<OptimizedProgram, OptimizeError>
{
    if input.ast.len() != input.ir.len() {
        return Err(OptimizeError::InvalidArity);
    }
    let original_nodes = measure(&input.ir)?;
    // Do not trust an arbitrary caller-built IR just because each role is
    // individually genuine. The IR must be EXACTLY the SENS-law lowering
    // of the AST that accompanies it, in the original root order.
    for (ast, supplied_ir) in input.ast.iter().zip(&input.ir) {
        let proved_ir = lower(ast).map_err(|_| OptimizeError::UnprovedRole)?;
        if &proved_ir != supplied_ir {
            return Err(OptimizeError::AstIrMismatch);
        }
    }
    let mut witnesses = Vec::new();
    let ir: Vec<ModernIr> = input.ir.iter()
        .map(|node| rewrite_verified(node, &mut witnesses)).collect();
    let optimized_nodes = measure(&ir)?;
    if optimized_nodes > original_nodes || (optimized_nodes == original_nodes && !witnesses.is_empty()) {
        return Err(OptimizeError::NodeLimitExceeded);
    }
    if witnesses.iter().any(|w| w.before_nodes <= w.after_nodes) {
        return Err(OptimizeError::NodeLimitExceeded);
    }
    Ok(OptimizedProgram {
        canonical_t5_sha256: input.canonical_t5_sha256,
        original_nodes, optimized_nodes, ir, witnesses,
    })
}

/// Operator-facing opt-in entry: physical exact T5 -> canonical SENS-owned
/// parser + admitted domains -> original verified CML IR -> O1 optimizer.
/// This can NEVER infer domains from packed bytes or human spellings, and
/// it does not alter or overwrite the original physical file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizePipelineError {
    InvalidPhysicalOrSemanticSource(ModernLowerError),
    UnsoundOrUnverifiedRewrite(OptimizeError),
}

pub fn optimize_canonical_t5(physical_t5: &[u8])
    -> Result<OptimizedProgram, OptimizePipelineError>
{
    let source = lower_canonical_t5_bytes(physical_t5)
        .map_err(OptimizePipelineError::InvalidPhysicalOrSemanticSource)?;
    optimize_exact_ir(&source)
        .map_err(OptimizePipelineError::UnsoundOrUnverifiedRewrite)
}

/// Replay against the original SENS-law-verified IR and compare the complete
/// witness/output, including the physical source digest. This is a
/// deterministic witness consistency check; independent semantic equivalence
/// is established separately by SENS-oracle differential tests.
pub fn replay_optimizer_witness(
    source: &VerifiedProgramIr,
    candidate: &OptimizedProgram,
) -> Result<bool, OptimizeError> {
    Ok(optimize_exact_ir(source)? == *candidate)
}

/// Independently replay a claimed optimization against the ORIGINAL canonical
/// physical T5 bytes, not caller-provided AST/IR metadata. This binds the
/// purported source digest, role law, IR and ordered rewrite witnesses to the
/// same actual file bytes and prevents cross-program witness substitution.
///
/// A successful replay is a deterministic compiler-integrity check, NOT a
/// general proof of semantic equivalence. The two admitted rewrites still
/// require the independent SENS-oracle differential test corpus.
pub fn replay_optimizer_witness_from_t5(
    original_physical_t5: &[u8],
    candidate: &OptimizedProgram,
) -> Result<bool, OptimizePipelineError> {
    // No auto-detection of F3. The canonical T5 reader rejects mismatching
    // formats, truncation and noncanonical bytes before trusting the witness.
    let original = lower_canonical_t5_bytes(original_physical_t5)
        .map_err(OptimizePipelineError::InvalidPhysicalOrSemanticSource)?;
    if candidate.canonical_t5_sha256 != sens::sha256_source(original_physical_t5) {
        return Ok(false);
    }
    let freshly_optimized = optimize_exact_ir(&original)
        .map_err(OptimizePipelineError::UnsoundOrUnverifiedRewrite)?;
    Ok(freshly_optimized == *candidate)
}

/// A typed IR interface can be used from any backend without granting it
/// permission to change domain semantics; only concrete target selection is
/// backend-owned. SBCL's IR staging inspires the separation, not the language
/// definitions.
pub fn is_admitted_rule(role: CompilerLoweringRole) -> bool {
    matches!(role,
        CompilerLoweringRole::SelectorHead
        | CompilerLoweringRole::SelectorTail
        | CompilerLoweringRole::PairConstruct
        | CompilerLoweringRole::QuoteForm
    )
}

// Prevent unused imports from drifting into a new registry of numeric codes.
const _: () = { let _ = is_admitted_rule as fn(CompilerLoweringRole) -> bool; };
