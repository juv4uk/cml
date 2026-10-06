//! Current SENS source -> CML IR bridge for the selfhost compiler nucleus.
//!
//! This path consumes SENS' own parser/lowerer so callable heads arrive as
//! exact DomainCall values. It never consults CML's legacy surface/Sid8
//! lowering table for current source.

use crate::compiler_mechanism::RichCompilerMechanismRef;
use crate::ir::{Ir, Params, PrimOp, Quoted};
use crate::sens_domain_bridge::AuthorityProvenance;
use crate::sens_rich_bridge::VerifiedRichMechanism;
use sens::syntax::{Exactness, Expr, ExprKind};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct CurrentSensProgram {
    pub ir: Vec<Ir>,
    pub authority: AuthorityProvenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedMechanismRegistry {
    entries: Vec<VerifiedRichMechanism>,
    authority: AuthorityProvenance,
}

impl VerifiedMechanismRegistry {
    pub fn from_verified(
        entries: Vec<VerifiedRichMechanism>,
    ) -> Result<Self, CurrentLowerError> {
        let Some(first) = entries.first() else {
            return Err(CurrentLowerError::EmptyVerifiedExport);
        };
        let authority = first.provenance().clone();

        for (index, entry) in entries.iter().enumerate() {
            if entry.provenance() != &authority {
                return Err(CurrentLowerError::MixedVerifiedAuthority);
            }
            if entries[..index]
                .iter()
                .any(|prior| prior.identity() == entry.identity())
            {
                return Err(CurrentLowerError::DuplicateVerifiedIdentity);
            }
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

    fn mechanism(
        &self,
        identity: sens::DomainIdentity,
    ) -> Result<RichCompilerMechanismRef, CurrentLowerError> {
        self.entries
            .iter()
            .find(|entry| entry.identity() == identity)
            .map(VerifiedRichMechanism::mechanism_ref)
            .ok_or(CurrentLowerError::MissingVerifiedIdentity)
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
    EmptyVerifiedExport,
    MixedVerifiedAuthority,
    DuplicateVerifiedIdentity,
    MissingVerifiedIdentity,
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
            Self::EmptyVerifiedExport => {
                write!(f, "verified SENS compiler export is empty")
            }
            Self::MixedVerifiedAuthority => {
                write!(f, "verified compiler mechanisms carry mixed SENS authority")
            }
            Self::DuplicateVerifiedIdentity => {
                write!(f, "verified compiler export contains a duplicate exact identity")
            }
            Self::MissingVerifiedIdentity => {
                write!(f, "exact DomainIdentity is absent from the verified SENS export")
            }
        }
    }
}

impl std::error::Error for CurrentLowerError {}

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

fn lower_lambda(
    arguments: &[Expr],
    registry: &VerifiedMechanismRegistry,
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
    registry: &VerifiedMechanismRegistry,
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

fn mechanism_arity(
    mechanism: RichCompilerMechanismRef,
    arguments: &[Expr],
) -> Result<(), CurrentLowerError> {
    let (name, expected) = match mechanism {
        RichCompilerMechanismRef::AtomPredicateD1
        | RichCompilerMechanismRef::SelectorTail
        | RichCompilerMechanismRef::SelectorHead => ("unary compiler mechanism", 1),
        RichCompilerMechanismRef::AtomEqualityD1
        | RichCompilerMechanismRef::PairConstruct => ("binary compiler mechanism", 2),
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
    registry: &VerifiedMechanismRegistry,
) -> Result<Ir, CurrentLowerError> {
    let mechanism = registry.mechanism(identity)?;
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
                .map(|item| lower_expr(item, registry))
                .collect::<Result<Vec<_>, _>>()?,
        }),
    }
}

/// Lower the complete current SENS source to CML IR without legacy semantic routing.
pub fn lower_current_sens_source(
    source: &str,
    registry: &VerifiedMechanismRegistry,
) -> Result<CurrentSensProgram, CurrentLowerError> {
    let parsed =
        sens::parse(source).map_err(|error| CurrentLowerError::Parse(error.to_string()))?;
    let lowered = sens::lower_program(&parsed);

    Ok(CurrentSensProgram {
        ir: lowered
            .iter()
            .map(|item| lower_expr(item, registry))
            .collect::<Result<Vec<_>, _>>()?,
        authority: registry.authority().clone(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn production_lowering_consumes_export_verified_registry_only() {
        let source = include_str!("sens_current_lowering.rs");
        let forbidden = [
            ["verify_current_", "identity("].concat(),
            ["compiler_lowering_role_", "from_sens("].concat(),
            ["packed_", "bits("].concat(),
            ["Sid", "8"].concat(),
        ];
        for needle in forbidden {
            assert!(
                !source.contains(&needle),
                "current IR lowering reintroduced semantic admission: {needle}"
            );
        }
        assert!(source.contains("registry.mechanism(identity)?"));
        assert!(source.contains("VerifiedMechanismRegistry"));
    }
}
