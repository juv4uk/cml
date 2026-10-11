//! Source-shared, modern exact SENS AST/IR data contract for CML.
//!
//! Compiler-role meanings MUST be proved by SENS executable laws before
//! instantiating these nodes; D3 and D4 coordinates can never be conflated.
//! No legacy SID8 table, number-based dispatch or unverified role conversion.
//! The isolated candidate compiler and the root CML AST/IR use this ONE source.
use sens::{CompilerLoweringRole, CoreDomainIdentity, DomainIdentity};

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

