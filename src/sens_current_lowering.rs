//! Current SENS source -> CML IR bridge for the selfhost compiler nucleus.
//!
//! This path consumes SENS' own parser/lowerer so callable heads arrive as
//! exact DomainCall values. It never consults CML's legacy surface/Sid8
//! lowering table for current source.

use crate::compiler_mechanism::{
    RichCompilerMechanismRef, select_rich_compiler_mechanism,
};
use crate::ir::{Ir, Params, PrimOp, Quoted};
use crate::sens_domain_bridge::{AuthorityProvenance, BridgeError, pinned_authority};
use sens::syntax::{Exactness, Expr, ExprKind};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct CurrentSensProgram {
    pub ir: Vec<Ir>,
    pub authority: AuthorityProvenance,
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
    UnknownRole,
    Authority(BridgeError),
}

impl fmt::Display for CurrentLowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "SENS current-source parse/lower failure: {message}"),
            Self::UnsupportedLegacyIdentity => {
                write!(f, "legacy Sid8/Call identity is forbidden in current SENS lowering")
            }
            Self::UnsupportedDomainIdentity => {
                write!(f, "bare DomainIdentity cannot execute outside a verified DomainCall")
            }
            Self::UnsupportedLiteral(kind) => {
                write!(f, "current SENS nucleus literal is unsupported in CML IR: {kind}")
            }
            Self::Arity { role, expected, actual } => {
                write!(f, "{role} expects {expected} argument(s), got {actual}")
            }
            Self::InvalidCondClause(index) => {
                write!(f, "current D3 COND clause {index} must contain exactly (test expression)")
            }
            Self::UnknownRole => write!(f, "current SENS compiler nucleus returned no admitted role"),
            Self::Authority(error) => write!(f, "SENS authority verification failed: {error}"),
        }
    }
}

impl std::error::Error for CurrentLowerError {}

impl From<BridgeError> for CurrentLowerError {
    fn from(error: BridgeError) -> Self {
        Self::Authority(error)
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
        ExprKind::BinaryNumber(_) => {
            Err(CurrentLowerError::UnsupportedLiteral("binary number"))
        }
        ExprKind::NumericBuffer(_) => {
            Err(CurrentLowerError::UnsupportedLiteral("numeric buffer"))
        }
        ExprKind::Sid(_) | ExprKind::Call(_, _) => Err(CurrentLowerError::UnsupportedLegacyIdentity),
        ExprKind::DomainIdentity(_) => Err(CurrentLowerError::UnsupportedDomainIdentity),
        ExprKind::DomainCall(_, _) => {
            Err(CurrentLowerError::UnsupportedLiteral("call in quote"))
        }
        ExprKind::Local { .. } => {
            Err(CurrentLowerError::UnsupportedLiteral("resolved local in quote"))
        }
    }
}

fn lower_expr(expr: &Expr) -> Result<Ir, CurrentLowerError> {
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
            let func = lower_expr(&items[0])?;
            let args = items[1..]
                .iter()
                .map(lower_expr)
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
        ExprKind::Sid(_) | ExprKind::Call(_, _) => Err(CurrentLowerError::UnsupportedLegacyIdentity),
        ExprKind::DomainIdentity(_) => Err(CurrentLowerError::UnsupportedDomainIdentity),
        ExprKind::DomainCall(identity, arguments) => {
            lower_domain_call((*identity).into(), arguments)
        },
        ExprKind::Local { .. } => Err(CurrentLowerError::UnsupportedLiteral("resolved local")),
    }
}

fn lower_lambda(arguments: &[Expr]) -> Result<Ir, CurrentLowerError> {
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
        _ => return Err(CurrentLowerError::UnsupportedLiteral("lambda parameter list")),
    };

    let body = lower_expr(&arguments[1])?;
    Ok(Ir::Lambda {
        params,
        body: Box::new(body),
    })
}

fn lower_define(arguments: &[Expr]) -> Result<Ir, CurrentLowerError> {
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
        value: Box::new(lower_expr(&arguments[1])?),
    })
}

fn lower_cond(arguments: &[Expr]) -> Result<Ir, CurrentLowerError> {
    let mut flattened = Vec::with_capacity(arguments.len() * 2);
    for (index, clause) in arguments.iter().enumerate() {
        let ExprKind::List(parts) = &clause.kind else {
            return Err(CurrentLowerError::InvalidCondClause(index));
        };
        if parts.len() != 2 {
            return Err(CurrentLowerError::InvalidCondClause(index));
        }
        flattened.push(lower_expr(&parts[0])?);
        flattened.push(lower_expr(&parts[1])?);
    }

    Ok(Ir::Prim {
        op: PrimOp::CompilerConditionalExactD1(RichCompilerMechanismRef::ConditionalD1),
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
        sens::CompilerLoweringRole::AtomEquality
        | sens::CompilerLoweringRole::PairConstruct => ("binary compiler mechanism", 2),
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
) -> Result<Ir, CurrentLowerError> {
    let core = identity
        .core_operation()
        .ok_or(CurrentLowerError::UnknownRole)?;
    let role = sens::compiler_lowering_role_from_sens(core)
        .map_err(|error| CurrentLowerError::Parse(error.to_string()))?
        .ok_or(CurrentLowerError::UnknownRole)?;

    primitive_arity(role, arguments)?;
    let mechanism = select_rich_compiler_mechanism(role);

    match role {
        sens::CompilerLoweringRole::QuoteForm => Ok(Ir::Quote(lower_quoted(&arguments[0])?)),
        sens::CompilerLoweringRole::LambdaForm => lower_lambda(arguments),
        sens::CompilerLoweringRole::DefineForm => lower_define(arguments),
        sens::CompilerLoweringRole::CondForm => lower_cond(arguments),
        sens::CompilerLoweringRole::AtomPredicate
        | sens::CompilerLoweringRole::SelectorTail
        | sens::CompilerLoweringRole::SelectorHead
        | sens::CompilerLoweringRole::AtomEquality
        | sens::CompilerLoweringRole::PairConstruct => Ok(Ir::Prim {
            op: PrimOp::CompilerMechanism(mechanism),
            args: arguments
                .iter()
                .map(lower_expr)
                .collect::<Result<Vec<_>, _>>()?,
        }),
    }
}

/// Lower the complete current SENS source to CML IR without legacy semantic routing.
pub fn lower_current_sens_source(source: &str) -> Result<CurrentSensProgram, CurrentLowerError> {
    let authority = pinned_authority()?;
    let parsed = sens::parse(source)
        .map_err(|error| CurrentLowerError::Parse(error.to_string()))?;
    let lowered = sens::lower_program(&parsed);

    Ok(CurrentSensProgram {
        ir: lowered
            .iter()
            .map(lower_expr)
            .collect::<Result<Vec<_>, _>>()?,
        authority,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NUCLEUS: &str = include_str!("../external/sens/lib/compiler-nucleus.lisp");

    #[test]
    fn current_nucleus_lowers_without_legacy_identity() {
        let program = lower_current_sens_source(NUCLEUS)
            .expect("current nucleus must lower");
        assert!(!program.ir.is_empty());

        fn walk(ir: &Ir, count: &mut usize) {
            match ir {
                Ir::Prim { op, args } => {
                    if matches!(
                        op,
                        PrimOp::CompilerMechanism(_)
                            | PrimOp::CompilerConditionalExactD1(_)
                    ) {
                        *count += 1;
                    }
                    for arg in args {
                        walk(arg, count);
                    }
                }
                Ir::Lambda { body, .. } => walk(body, count),
                Ir::Def { value, .. } => walk(value, count),
                Ir::App { func, args } => {
                    walk(func, count);
                    for arg in args {
                        walk(arg, count);
                    }
                }
                Ir::Let { body, .. } => walk(body, count),
                Ir::Cond { branches } => {
                    for (a, b) in branches {
                        walk(a, count);
                        walk(b, count);
                    }
                }
                Ir::CondMatch { branches } => {
                    for (a, _, b) in branches {
                        walk(a, count);
                        walk(b, count);
                    }
                }
                Ir::Quote(_)
                | Ir::Int(_)
                | Ir::Float(_)
                | Ir::Rational(_, _)
                | Ir::String(_)
                | Ir::Buffer(_)
                | Ir::Nil
                | Ir::True
                | Ir::Var(_)
                | Ir::Builtin(_)
                | Ir::Sid(_)
                | Ir::MachinePrim { .. }
                | Ir::TailSelfCall { .. } => {}
            }
        }

        let mut mechanism_count = 0;
        for ir in &program.ir {
            walk(ir, &mut mechanism_count);
        }
        assert!(
            mechanism_count >= 20,
            "nucleus did not reach enough current mechanisms"
        );
    }

    #[test]
    fn wrong_domain_fails_closed() {
        let wrong = sens::syntax::Expr {
            kind: ExprKind::DomainCall(
                sens::CoreDomainIdentity::D4(sens::CoreD4::from_word(
                    sens::Bit4::new(0b0111).unwrap(),
                )),
                std::rc::Rc::from([]),
            ),
            span: sens::Span::default(),
        };
        assert!(matches!(
            lower_domain_call(
                match wrong.kind {
                    ExprKind::DomainCall(identity, _) => identity,
                    _ => unreachable!(),
                },
                &[],
            ),
            Err(CurrentLowerError::UnknownRole)
        ));
    }
}
