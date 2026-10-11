//! Source-shared, modern exact SENS AST/IR data contract for CML.
//!
//! Compiler-role meanings MUST be proved by SENS executable laws before
//! instantiating these nodes; D3 and D4 coordinates can never be conflated.
//! No legacy SID8 table, number-based dispatch or unverified role conversion.
//! The isolated candidate compiler and the root CML AST/IR use this ONE source.
use sens::{CompilerLoweringRole, CoreDomainIdentity, DomainIdentity};
use sens::syntax::{Expr, ExprKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedRole {
    pub identity: CoreDomainIdentity,
    pub role: CompilerLoweringRole,
    pub law_ref: &'static str,
    pub proof_ref: &'static str,
    pub authority_path: &'static str,
    pub authority_sha256: String,
    pub contract_version: &'static str,
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactQuotedData {
    EmptyList,
    DomainWord(DomainIdentity),
    ProperList(Vec<ExactQuotedData>),
    DottedPair(Box<ExactQuotedData>, Box<ExactQuotedData>),
}


/// Intermediate syntax owned by CML: every call includes a SENS-verified
/// compiler-law proof, and a quoted payload is never recursively executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModernAst {
    Literal(ExactQuotedData),
    Quote { binding: VerifiedRole, data: ExactQuotedData },
    Call { binding: VerifiedRole, args: Vec<ModernAst> },
    Conditional { binding: VerifiedRole, branches: Vec<(ModernAst, ModernAst)> },
}

/// Target mechanisms are CML-private, selected from ALREADY SENS-verified
/// lowering roles. No CML table associates numeric domain codes with meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmlMechanism {
    AtomPredicate, Head, Tail, AtomEquality, PairConstruct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModernIr {
    Literal(ExactQuotedData),
    Quote { binding: VerifiedRole, data: ExactQuotedData },
    Mechanism { binding: VerifiedRole, target: CmlMechanism, args: Vec<ModernIr> },
    Conditional { binding: VerifiedRole, branches: Vec<(ModernIr, ModernIr)> },
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedProgramIr {
    /// Every leaf is an exact D1 value or a quoted D1..D9 word/structure.
    /// Callable nodes have verified proof/ref and a CML-private mechanism.
    pub ast: Vec<ModernAst>,
    pub ir: Vec<ModernIr>,
    pub canonical_t5_sha256: [u8; 32],
}


// The ACTUAL current-domain AST quote converter and CML-private IR lowering.
// Quoted data is never treated as a function call; only SENS-verified roles
// are permitted in compiler call nodes. These functions are shared with CML.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModernLowerError {
    InvalidCanonicalSource,
    NoExecutableRoot,
    NonCallableHead,
    UnsupportedLiteral,
    UnsupportedForm,
    InvalidArity,
    InvalidConditionClause,
    StructuralDepthExceeded,
    UnverifiedCompilerRole,
    InvalidCompilerAuthority,
    InvalidGeneratedCorpus,
}


pub fn quoted_data(source: &Expr, depth: usize) -> Result<ExactQuotedData, ModernLowerError> {
    if depth > 128 { return Err(ModernLowerError::StructuralDepthExceeded); }
    match &source.kind {
        ExprKind::List(items) if items.is_empty() => Ok(ExactQuotedData::EmptyList),
        ExprKind::List(items) => Ok(ExactQuotedData::ProperList(items.iter()
            .map(|x| quoted_data(x, depth + 1))
            .collect::<Result<Vec<_>, _>>()?)),
        ExprKind::DomainIdentity(id) => Ok(ExactQuotedData::DomainWord(*id)),
        ExprKind::Pair(a, b) => Ok(ExactQuotedData::DottedPair(
            Box::new(quoted_data(a, depth + 1)?),
            Box::new(quoted_data(b, depth + 1)?),
        )),
        _ => Err(ModernLowerError::UnsupportedLiteral),
    }
}

pub fn lower(ast: &ModernAst) -> Result<ModernIr, ModernLowerError> {
    match ast {
        ModernAst::Literal(data) => Ok(ModernIr::Literal(data.clone())),
        ModernAst::Quote {binding, data} => Ok(ModernIr::Quote {
            binding: binding.clone(), data: data.clone()
        }),
        ModernAst::Call {binding, args} => {
            let target = match binding.role {
                CompilerLoweringRole::AtomPredicate => CmlMechanism::AtomPredicate,
                CompilerLoweringRole::SelectorHead => CmlMechanism::Head,
                CompilerLoweringRole::SelectorTail => CmlMechanism::Tail,
                CompilerLoweringRole::AtomEquality => CmlMechanism::AtomEquality,
                CompilerLoweringRole::PairConstruct => CmlMechanism::PairConstruct,
                _ => return Err(ModernLowerError::UnsupportedForm),
            };
            Ok(ModernIr::Mechanism {
                binding: binding.clone(), target,
                args: args.iter().map(lower).collect::<Result<Vec<_>, _>>()?,
            })
        }
        ModernAst::Conditional {binding, branches} => Ok(ModernIr::Conditional {
            binding: binding.clone(),
            branches: branches.iter().map(|(x, y)| Ok((lower(x)?, lower(y)?)))
                .collect::<Result<Vec<_>, ModernLowerError>>()?,
        }),
    }
}


// SENS owns the D3/D4 compiler law. Only a pinned, executable law +
// exact contract digest can authorize one callable AST head.
pub fn verified_role(identity: CoreDomainIdentity) -> Result<VerifiedRole, ModernLowerError> {
    // No role, proof or semantics are inferred from width / bits / name.
    let record = sens::compiler_semantic_input_from_sens(identity)
        .map_err(|_| ModernLowerError::UnverifiedCompilerRole)?
        .ok_or(ModernLowerError::UnverifiedCompilerRole)?;
    if record.identity != identity || record.semantic_status != "current"
        || record.authority_ref.is_empty() || record.proof_ref.is_empty()
        || record.authority_path != "language-contract.lisp"
        || record.language_contract_version.is_empty()
        || record.authority_sha256.len() != 64
    {
        return Err(ModernLowerError::InvalidCompilerAuthority);
    }
    let pinned_authority = sens::sha256_source(
        include_bytes!("../external/sens/language-contract.lisp"),
    ).iter().map(|b| format!("{b:02x}")).collect::<String>();
    if record.authority_sha256 != pinned_authority {
        return Err(ModernLowerError::InvalidCompilerAuthority);
    }
    Ok(VerifiedRole {
        identity, role: record.lowering_role,
        law_ref: record.authority_ref, proof_ref: record.proof_ref,
        authority_path: record.authority_path,
        authority_sha256: record.authority_sha256,
        contract_version: record.language_contract_version,
    })
}


// Build executable current-domain AST ONLY through the above SENS role.
// Quoted residual structures remain data; unsupported binders fail closed.
pub fn exact_call(source: &Expr, depth: usize) -> Result<ModernAst, ModernLowerError> {
    if depth > 128 { return Err(ModernLowerError::StructuralDepthExceeded); }
    match &source.kind {
        ExprKind::List(items) if items.is_empty() =>
            Ok(ModernAst::Literal(ExactQuotedData::EmptyList)),
        ExprKind::DomainIdentity(DomainIdentity::D1(_)) =>
            Ok(ModernAst::Literal(quoted_data(source, depth)?)),
        ExprKind::DomainCall(identity, args) => {
            let binding = verified_role(*identity)?;
            match binding.role {
                CompilerLoweringRole::QuoteForm => {
                    if args.len() != 1 { return Err(ModernLowerError::InvalidArity); }
                    Ok(ModernAst::Quote {
                        binding,
                        data: quoted_data(&args[0], depth + 1)?,
                    })
                }
                CompilerLoweringRole::CondForm => {
                    let mut branches = Vec::with_capacity(args.len());
                    for clause in args.iter() {
                        let ExprKind::List(parts) = &clause.kind else {
                            return Err(ModernLowerError::InvalidConditionClause);
                        };
                        if parts.len() != 2 { return Err(ModernLowerError::InvalidConditionClause); }
                        branches.push((exact_call(&parts[0], depth + 1)?,
                                       exact_call(&parts[1], depth + 1)?));
                    }
                    Ok(ModernAst::Conditional { binding, branches })
                }
                CompilerLoweringRole::LambdaForm | CompilerLoweringRole::DefineForm =>
                    Err(ModernLowerError::UnsupportedForm), // binder/locals proof not yet migrated
                _ => {
                    let arity = match binding.role {
                        CompilerLoweringRole::AtomPredicate
                        | CompilerLoweringRole::SelectorHead
                        | CompilerLoweringRole::SelectorTail => 1,
                        CompilerLoweringRole::AtomEquality
                        | CompilerLoweringRole::PairConstruct => 2,
                        _ => return Err(ModernLowerError::UnsupportedForm),
                    };
                    if args.len() != arity { return Err(ModernLowerError::InvalidArity); }
                    Ok(ModernAst::Call {
                        binding,
                        args: args.iter().map(|a| exact_call(a, depth + 1))
                            .collect::<Result<Vec<_>, _>>()?,
                    })
                }
            }
        }
        // For a raw D2 List with a non-callable head, reject instead of
        // inventing a callable identity ("(((0)))" is not an executable call).
        ExprKind::List(_) => Err(ModernLowerError::NonCallableHead),
        _ => Err(ModernLowerError::UnsupportedLiteral),
    }
}

