//! Verified current SENS source -> CML mechanism IR for the selfhost nucleus.
//!
//! Production admission is deliberately upstream of this module:
//!
//!   pinned SENS compiler-semantic-input/1
//!     -> parse_compiler_export
//!     -> verify_exported_request
//!     -> VerifiedCurrentRegistry
//!     -> exact DomainCall lookup
//!     -> CML-private mechanism IR
//!
//! This module never derives language meaning from DomainIdentity coordinates,
//! surface names, Sid8/Sens8, or a second compiler-role table.

use crate::compiler_mechanism::RichCompilerMechanismRef;
use crate::ir::{Ir, Params, PrimOp, Quoted};
use crate::sens_compiler_export::{
    CompilerExportError, parse_compiler_export, verify_exported_request,
};
use crate::sens_domain_bridge::AuthorityProvenance;
use crate::sens_rich_bridge::VerifiedRichMechanism;
use sens::syntax::{Exactness, Expr, ExprKind};
use std::fmt;

const CURRENT_COMPILER_CLOSURE_SIZE: usize = 9;

#[derive(Debug, Clone, PartialEq)]
pub struct CurrentSensProgram {
    pub ir: Vec<Ir>,
    pub authority: AuthorityProvenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedCurrentRegistry {
    entries: Vec<VerifiedRichMechanism>,
    authority: AuthorityProvenance,
}

impl VerifiedCurrentRegistry {
    /// Build the only production mechanism registry from the real SENS export.
    pub fn from_export(text: &str) -> Result<Self, CurrentLowerError> {
        let exported = parse_compiler_export(text)?;
        if exported.len() != CURRENT_COMPILER_CLOSURE_SIZE {
            return Err(CurrentLowerError::UnexpectedVerifiedClosureSize {
                expected: CURRENT_COMPILER_CLOSURE_SIZE,
                actual: exported.len(),
            });
        }

        let mut entries = Vec::with_capacity(exported.len());
        for request in exported {
            let verified = verify_exported_request(request)?;
            if entries
                .iter()
                .any(|entry: &VerifiedRichMechanism| entry.identity() == verified.identity())
            {
                return Err(CurrentLowerError::DuplicateVerifiedIdentity);
            }
            entries.push(verified);
        }

        let authority = entries
            .first()
            .expect("nine-role compiler export is non-empty")
            .provenance()
            .clone();
        if entries.iter().any(|entry| entry.provenance() != &authority) {
            return Err(CurrentLowerError::MixedVerifiedAuthority);
        }

        Ok(Self { entries, authority })
    }

    fn lookup(
        &self,
        identity: sens::DomainIdentity,
    ) -> Result<&VerifiedRichMechanism, CurrentLowerError> {
        self.entries
            .iter()
            .find(|entry| entry.identity() == identity)
            .ok_or(CurrentLowerError::MissingVerifiedIdentity)
    }

    pub fn authority(&self) -> &AuthorityProvenance {
        &self.authority
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentLowerError {
    Parse(String),
    Export(CompilerExportError),
    UnsupportedLegacyIdentity,
    UnsupportedDomainIdentity,
    UnsupportedLiteral(&'static str),
    Arity {
        role: &'static str,
        expected: usize,
        actual: usize,
    },
    InvalidCondClause(usize),
    UnexpectedVerifiedClosureSize {
        expected: usize,
        actual: usize,
    },
    DuplicateVerifiedIdentity,
    MixedVerifiedAuthority,
    MissingVerifiedIdentity,
}

impl fmt::Display for CurrentLowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "SENS current-source parse/lower failure: {message}"),
            Self::Export(error) => write!(f, "SENS compiler export rejected: {error}"),
            Self::UnsupportedLegacyIdentity => {
                write!(
                    f,
                    "legacy Sid8/Call identity is forbidden in current SENS lowering"
                )
            }
            Self::UnsupportedDomainIdentity => {
                write!(
                    f,
                    "bare DomainIdentity cannot execute outside a verified DomainCall"
                )
            }
            Self::UnsupportedLiteral(kind) => {
                write!(
                    f,
                    "current SENS nucleus literal is unsupported in CML IR: {kind}"
                )
            }
            Self::Arity {
                role,
                expected,
                actual,
            } => write!(f, "{role} expects {expected} argument(s), got {actual}"),
            Self::InvalidCondClause(index) => {
                write!(
                    f,
                    "current D3 COND clause {index} must contain exactly (test expression)"
                )
            }
            Self::UnexpectedVerifiedClosureSize { expected, actual } => {
                write!(
                    f,
                    "verified compiler export contains {actual} identities, expected {expected}"
                )
            }
            Self::DuplicateVerifiedIdentity => {
                write!(
                    f,
                    "verified compiler export contains a duplicate exact identity"
                )
            }
            Self::MixedVerifiedAuthority => {
                write!(f, "verified compiler export mixes authority provenance")
            }
            Self::MissingVerifiedIdentity => {
                write!(
                    f,
                    "current DomainCall identity is absent from the verified SENS compiler export"
                )
            }
        }
    }
}

impl std::error::Error for CurrentLowerError {}

impl From<CompilerExportError> for CurrentLowerError {
    fn from(error: CompilerExportError) -> Self {
        Self::Export(error)
    }
}

fn exact_int(value: f64) -> Option<i64> {
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    if value < i64::MIN as f64 || value > i64::MAX as f64 {
        return None;
    }
    Some(value as i64)
}

fn symbol_name(symbol: &str) -> Ir {
    Ir::Var(symbol.to_uppercase())
}

fn lower_quoted(expr: &Expr) -> Result<Quoted, CurrentLowerError> {
    match &expr.kind {
        ExprKind::Number(value, _) => exact_int(*value)
            .map(Quoted::Int)
            .ok_or(CurrentLowerError::UnsupportedLiteral("non-integer number")),
        ExprKind::Rational(_) => Err(CurrentLowerError::UnsupportedLiteral("rational")),
        ExprKind::String(value) => Ok(Quoted::Str(value.to_string())),
        // In the current SENS path a quoted symbol is semantic data, not
        // a CML target identifier. Preserve its exact source bytes so the
        // compiled self-host artifact has the same canonical evidence as the
        // SENS oracle. Generic CML lowering may keep its legacy identifier
        // normalization; this authority path must not.
        ExprKind::Symbol(symbol) => Ok(Quoted::Sym {
            uppercased: symbol.to_string(),
            original: symbol.to_string(),
        }),
        ExprKind::List(items) => Ok(Quoted::List(
            items
                .iter()
                .map(lower_quoted)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        ExprKind::Pair(head, tail) => Ok(Quoted::DottedList(
            vec![lower_quoted(head)?],
            Box::new(lower_quoted(tail)?),
        )),
        ExprKind::BinaryNumber(_) => Err(CurrentLowerError::UnsupportedLiteral("binary number")),
        ExprKind::NumericBuffer(_) => Err(CurrentLowerError::UnsupportedLiteral("numeric buffer")),
        ExprKind::Sid(_) | ExprKind::Call(_, _) => {
            Err(CurrentLowerError::UnsupportedLegacyIdentity)
        }
        ExprKind::DomainIdentity(_) => Err(CurrentLowerError::UnsupportedDomainIdentity),
        ExprKind::DomainCall(_, _) => Err(CurrentLowerError::UnsupportedLiteral("call in quote")),
        ExprKind::Local { .. } => Err(CurrentLowerError::UnsupportedLiteral(
            "resolved local in quote",
        )),
    }
}

fn lower_expr(expr: &Expr, registry: &VerifiedCurrentRegistry) -> Result<Ir, CurrentLowerError> {
    match &expr.kind {
        ExprKind::Number(value, _) => {
            if let Some(value) = exact_int(*value) {
                Ok(Ir::Int(value))
            } else if value.is_finite() {
                Ok(Ir::Float(*value))
            } else {
                Err(CurrentLowerError::UnsupportedLiteral("number"))
            }
        }
        ExprKind::Rational(_) => Err(CurrentLowerError::UnsupportedLiteral("rational")),
        ExprKind::String(value) => Ok(Ir::String(value.to_string())),
        ExprKind::Symbol(symbol) => Ok(symbol_name(symbol)),
        ExprKind::List(items) => {
            if items.is_empty() {
                return Ok(Ir::Nil);
            }
            let func = lower_expr(&items[0], registry)?;
            let args = items[1..]
                .iter()
                .map(|arg| lower_expr(arg, registry))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Ir::App {
                func: Box::new(func),
                args,
            })
        }
        ExprKind::Pair(_, _) => Err(CurrentLowerError::UnsupportedLiteral(
            "executable dotted pair",
        )),
        ExprKind::BinaryNumber(_) => Err(CurrentLowerError::UnsupportedLiteral("binary number")),
        ExprKind::NumericBuffer(_) => Err(CurrentLowerError::UnsupportedLiteral("numeric buffer")),
        ExprKind::Sid(_) | ExprKind::Call(_, _) => {
            Err(CurrentLowerError::UnsupportedLegacyIdentity)
        }
        ExprKind::DomainIdentity(_) => Err(CurrentLowerError::UnsupportedDomainIdentity),
        ExprKind::DomainCall(identity, arguments) => {
            lower_domain_call((*identity).into(), arguments, registry)
        }
        ExprKind::Local { .. } => Err(CurrentLowerError::UnsupportedLiteral("resolved local")),
    }
}

fn lower_lambda(
    arguments: &[Expr],
    registry: &VerifiedCurrentRegistry,
) -> Result<Ir, CurrentLowerError> {
    if arguments.len() != 2 {
        return Err(CurrentLowerError::Arity {
            role: "LambdaForm",
            expected: 2,
            actual: arguments.len(),
        });
    }

    let params = match &arguments[0].kind {
        ExprKind::List(items) => Params::Fixed(
            items
                .iter()
                .map(|item| match &item.kind {
                    ExprKind::Symbol(symbol) => Ok(symbol.to_uppercase()),
                    _ => Err(CurrentLowerError::UnsupportedLiteral("lambda parameter")),
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        _ => {
            return Err(CurrentLowerError::UnsupportedLiteral(
                "lambda parameter list",
            ));
        }
    };

    let body = lower_expr(&arguments[1], registry)?;
    Ok(Ir::Lambda {
        params,
        body: Box::new(body),
    })
}

fn lower_define(
    arguments: &[Expr],
    registry: &VerifiedCurrentRegistry,
) -> Result<Ir, CurrentLowerError> {
    if arguments.len() != 2 {
        return Err(CurrentLowerError::Arity {
            role: "DefineForm",
            expected: 2,
            actual: arguments.len(),
        });
    }
    let name = match &arguments[0].kind {
        ExprKind::Symbol(symbol) => symbol.to_uppercase(),
        _ => return Err(CurrentLowerError::UnsupportedLiteral("define name")),
    };
    Ok(Ir::Def {
        name,
        value: Box::new(lower_expr(&arguments[1], registry)?),
    })
}

fn lower_cond(
    arguments: &[Expr],
    mechanism: RichCompilerMechanismRef,
    registry: &VerifiedCurrentRegistry,
) -> Result<Ir, CurrentLowerError> {
    let mut flattened = Vec::with_capacity(arguments.len() * 2);
    for (index, clause) in arguments.iter().enumerate() {
        let ExprKind::List(parts) = &clause.kind else {
            return Err(CurrentLowerError::InvalidCondClause(index));
        };
        if parts.len() != 2 {
            return Err(CurrentLowerError::InvalidCondClause(index));
        }
        flattened.push(lower_expr(&parts[0], registry)?);
        flattened.push(lower_expr(&parts[1], registry)?);
    }

    Ok(Ir::Prim {
        op: PrimOp::CompilerConditionalExactD1(mechanism),
        args: flattened,
    })
}

fn mechanism_arity(
    mechanism: RichCompilerMechanismRef,
    arguments: &[Expr],
) -> Result<(), CurrentLowerError> {
    let (name, expected) = match mechanism {
        RichCompilerMechanismRef::AtomPredicateD1
        | RichCompilerMechanismRef::SelectorTail
        | RichCompilerMechanismRef::SelectorHead => ("unary compiler mechanism", 1),
        RichCompilerMechanismRef::AtomEqualityD1 | RichCompilerMechanismRef::PairConstruct => {
            ("binary compiler mechanism", 2)
        }
        RichCompilerMechanismRef::Quote => ("quote", 1),
        RichCompilerMechanismRef::ConditionalD1 => return Ok(()),
        RichCompilerMechanismRef::Lambda => ("lambda", 2),
        RichCompilerMechanismRef::Define => ("define", 2),
    };

    if arguments.len() == expected {
        Ok(())
    } else {
        Err(CurrentLowerError::Arity {
            role: name,
            expected,
            actual: arguments.len(),
        })
    }
}

fn lower_domain_call(
    identity: sens::DomainIdentity,
    arguments: &[Expr],
    registry: &VerifiedCurrentRegistry,
) -> Result<Ir, CurrentLowerError> {
    let verified = registry.lookup(identity)?;
    let mechanism = verified.mechanism_ref();

    mechanism_arity(mechanism, arguments)?;

    match mechanism {
        RichCompilerMechanismRef::Quote => Ok(Ir::Quote(lower_quoted(&arguments[0])?)),
        RichCompilerMechanismRef::Lambda => lower_lambda(arguments, registry),
        RichCompilerMechanismRef::Define => lower_define(arguments, registry),
        RichCompilerMechanismRef::ConditionalD1 => lower_cond(arguments, mechanism, registry),
        RichCompilerMechanismRef::AtomPredicateD1
        | RichCompilerMechanismRef::SelectorTail
        | RichCompilerMechanismRef::SelectorHead
        | RichCompilerMechanismRef::AtomEqualityD1
        | RichCompilerMechanismRef::PairConstruct => Ok(Ir::Prim {
            op: PrimOp::CompilerMechanism(mechanism),
            args: arguments
                .iter()
                .map(|arg| lower_expr(arg, registry))
                .collect::<Result<Vec<_>, _>>()?,
        }),
    }
}

/// Lower current SENS source only after constructing the registry from the
/// canonical proof-carrying compiler export.
pub fn lower_current_sens_source(
    source: &str,
    compiler_export: &str,
) -> Result<CurrentSensProgram, CurrentLowerError> {
    let registry = VerifiedCurrentRegistry::from_export(compiler_export)?;
    lower_current_sens_source_with_registry(source, &registry)
}

pub fn lower_current_sens_source_with_registry(
    source: &str,
    registry: &VerifiedCurrentRegistry,
) -> Result<CurrentSensProgram, CurrentLowerError> {
    let parsed =
        sens::parse(source).map_err(|error| CurrentLowerError::Parse(error.to_string()))?;
    let lowered = sens::lower_program(&parsed);

    Ok(CurrentSensProgram {
        ir: lowered
            .iter()
            .map(|expr| lower_expr(expr, registry))
            .collect::<Result<Vec<_>, _>>()?,
        authority: registry.authority().clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_source_has_no_semantic_reconstruction_route() {
        let source = include_str!("sens_current_lowering.rs");
        let forbidden = [
            ["verify_current", "_identity("].concat(),
            ["compiler_lowering_role_", "from_sens("].concat(),
            ["select_rich_compiler_", "mechanism("].concat(),
            ["packed", "_bits("].concat(),
        ];
        for forbidden in forbidden {
            assert!(
                !source.contains(&forbidden),
                "current production lowering reintroduced semantic reconstruction: {forbidden}"
            );
        }
        assert!(source.contains("verify_exported_request(request)?"));
        assert!(source.contains("registry.lookup(identity)?"));
    }

    #[test]
    fn current_quoted_symbol_preserves_exact_source_identity() {
        let expr = Expr {
            kind: ExprKind::Symbol(std::rc::Rc::from("quote-form")),
            span: sens::Span::default(),
        };
        assert_eq!(
            lower_quoted(&expr).unwrap(),
            Quoted::Sym {
                uppercased: "quote-form".to_string(),
                original: "quote-form".to_string(),
            }
        );
    }

    #[test]
    fn missing_verified_identity_fails_closed() {
        let registry = VerifiedCurrentRegistry {
            entries: Vec::new(),
            authority: crate::sens_domain_bridge::pinned_authority().unwrap(),
        };
        let identity =
            sens::DomainIdentity::D3(sens::Bija3::from_word(sens::Bit3::new(0b010).unwrap()));
        assert_eq!(
            lower_domain_call(identity, &[], &registry).unwrap_err(),
            CurrentLowerError::MissingVerifiedIdentity
        );
    }
}
