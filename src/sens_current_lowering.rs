//! Current SENS source -> CML IR bridge for the selfhost compiler nucleus.
//!
//! This path consumes SENS' own parser/lowerer so callable heads arrive as
//! exact DomainCall values. It never consults CML's legacy surface/Sid8
//! lowering table for current source.

use crate::compiler_mechanism::RichCompilerMechanismRef;
use crate::ir::{Ir, Params, PrimOp, Quoted};
use crate::sens_compiler_export::{
    CompilerExportError, parse_compiler_export, verify_exported_request,
};
use crate::sens_domain_bridge::AuthorityProvenance;
use crate::sens_rich_bridge::VerifiedRichMechanism;
use sens::syntax::{Exactness, Expr, ExprKind};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct CurrentSensProgram {
    pub ir: Vec<Ir>,
    pub authority: AuthorityProvenance,
}

#[derive(Debug, Clone)]
pub struct VerifiedMechanismRegistry {
    entries: Vec<VerifiedRichMechanism>,
    authority: AuthorityProvenance,
}

impl VerifiedMechanismRegistry {
    pub fn from_compiler_export(text: &str) -> Result<Self, CurrentLowerError> {
        let exported = parse_compiler_export(text)?;
        let mut entries = Vec::with_capacity(exported.len());

        for request in exported {
            let verified = verify_exported_request(request)?;
            if entries
                .iter()
                .any(|existing: &VerifiedRichMechanism| existing.identity() == verified.identity())
            {
                return Err(CurrentLowerError::DuplicateVerifiedIdentity(format!(
                    "{:?}",
                    verified.identity()
                )));
            }
            entries.push(verified);
        }

        let authority = entries
            .first()
            .ok_or(CurrentLowerError::EmptyVerifiedRegistry)?
            .provenance()
            .clone();

        if entries
            .iter()
            .any(|verified| verified.provenance() != &authority)
        {
            return Err(CurrentLowerError::MixedAuthorityRegistry);
        }

        Ok(Self { entries, authority })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn authority(&self) -> &AuthorityProvenance {
        &self.authority
    }

    fn get(&self, identity: sens::DomainIdentity) -> Option<&VerifiedRichMechanism> {
        self.entries
            .iter()
            .find(|verified| verified.identity() == identity)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentLowerError {
    Parse(String),
    UnsupportedLegacyIdentity,
    UnsupportedDomainIdentity,
    UnsupportedLiteral(&'static str),
    Arity {
        role: &'static str,
        expected: usize,
        actual: usize,
    },
    InvalidCondClause(usize),
    EmptyVerifiedRegistry,
    DuplicateVerifiedIdentity(String),
    MixedAuthorityRegistry,
    MissingVerifiedMechanism(String),
    Export(CompilerExportError),
}

impl fmt::Display for CurrentLowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "SENS current-source parse/lower failure: {message}"),
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
            } => {
                write!(f, "{role} expects {expected} argument(s), got {actual}")
            }
            Self::InvalidCondClause(index) => {
                write!(
                    f,
                    "current D3 COND clause {index} must contain exactly (test expression)"
                )
            }
            Self::EmptyVerifiedRegistry => {
                write!(f, "real SENS compiler export produced no verified mechanisms")
            }
            Self::DuplicateVerifiedIdentity(identity) => {
                write!(f, "duplicate exact identity in verified compiler export: {identity}")
            }
            Self::MixedAuthorityRegistry => {
                write!(f, "verified compiler export mixed authority provenance")
            }
            Self::MissingVerifiedMechanism(identity) => {
                write!(f, "current source identity is absent from verified SENS export: {identity}")
            }
            Self::Export(error) => {
                write!(f, "SENS compiler export verification failed: {error}")
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
        ExprKind::Symbol(symbol) => Ok(Quoted::Sym {
            uppercased: symbol.to_uppercase(),
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

fn lower_expr(expr: &Expr, registry: &VerifiedMechanismRegistry) -> Result<Ir, CurrentLowerError> {
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
                .map(|item| lower_expr(item, registry))
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

fn lower_lambda(arguments: &[Expr], registry: &VerifiedMechanismRegistry) -> Result<Ir, CurrentLowerError> {
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

fn lower_define(arguments: &[Expr], registry: &VerifiedMechanismRegistry) -> Result<Ir, CurrentLowerError> {
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
    registry: &VerifiedMechanismRegistry,
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

fn primitive_arity(
    role: sens::CompilerLoweringRole,
    arguments: &[Expr],
) -> Result<(), CurrentLowerError> {
    let (name, expected) = match role {
        sens::CompilerLoweringRole::AtomPredicate
        | sens::CompilerLoweringRole::SelectorTail
        | sens::CompilerLoweringRole::SelectorHead => ("unary compiler mechanism", 1),
        sens::CompilerLoweringRole::AtomEquality | sens::CompilerLoweringRole::PairConstruct => {
            ("binary compiler mechanism", 2)
        }
        sens::CompilerLoweringRole::QuoteForm => ("quote", 1),
        sens::CompilerLoweringRole::CondForm => return Ok(()),
        sens::CompilerLoweringRole::LambdaForm => ("lambda", 2),
        sens::CompilerLoweringRole::DefineForm => ("define", 2),
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
    registry: &VerifiedMechanismRegistry,
) -> Result<Ir, CurrentLowerError> {
    let verified = registry.get(identity).ok_or_else(|| {
        CurrentLowerError::MissingVerifiedMechanism(format!("{identity:?}"))
    })?;
    let role = verified.lowering_role();
    let mechanism = verified.mechanism_ref();

    primitive_arity(role, arguments)?;

    match role {
        sens::CompilerLoweringRole::QuoteForm => Ok(Ir::Quote(lower_quoted(&arguments[0])?)),
        sens::CompilerLoweringRole::LambdaForm => lower_lambda(arguments, registry),
        sens::CompilerLoweringRole::DefineForm => lower_define(arguments, registry),
        sens::CompilerLoweringRole::CondForm => lower_cond(arguments, mechanism, registry),
        sens::CompilerLoweringRole::AtomPredicate
        | sens::CompilerLoweringRole::SelectorTail
        | sens::CompilerLoweringRole::SelectorHead
        | sens::CompilerLoweringRole::AtomEquality
        | sens::CompilerLoweringRole::PairConstruct => Ok(Ir::Prim {
            op: PrimOp::CompilerMechanism(mechanism),
            args: arguments
                .iter()
                .map(|item| lower_expr(item, registry))
                .collect::<Result<Vec<_>, _>>()?;
        }),
    }
}

/// Build the production verified mechanism registry from the real
/// `compiler-semantic-input/1` transport, then lower current SENS source.
///
/// No production caller can lower a DomainCall without supplying the SENS
/// producer export that #622 verifies against the pinned authority.
pub fn lower_current_sens_source(
    source: &str,
    compiler_export: &str,
) -> Result<CurrentSensProgram, CurrentLowerError> {
    let registry = VerifiedMechanismRegistry::from_compiler_export(compiler_export)?;
    lower_current_sens_source_with_registry(source, &registry)
}

/// Mechanical source-shape -> IR lowering after semantic admission is complete.
pub fn lower_current_sens_source_with_registry(
    source: &str,
    registry: &VerifiedMechanismRegistry,
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
    fn production_lowering_contains_no_direct_role_admission_or_legacy_route() {
        let source = include_str!("sens_current_lowering.rs");
        for forbidden in [
            "verify_current_identity(",
            "compiler_lowering_role_from_sens(",
            "select_rich_compiler_mechanism(",
            "packed_bits(",
        ] {
            assert!(
                !source.contains(forbidden),
                "production lowering reintroduced forbidden admission route: {forbidden}"
            );
        }
        assert!(source.contains("verify_exported_request(request)?"));
        assert!(source.contains("registry.get(identity)"));
    }
}
