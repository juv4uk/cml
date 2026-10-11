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

