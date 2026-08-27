//! Fail-closed semantic admission between macro expansion and IR lowering.
//!
//! This is intentionally a small gate, not a claim of complete my-lisp
//! semantic analysis.  It rejects source shapes for which CML would otherwise
//! emit a different program than the canonical evaluator.

use crate::ast::Expr;
use std::collections::HashSet;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticErrorKind {
    DuplicateParameter,
    UnsupportedSequentialBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticError {
    pub kind: SemanticErrorKind,
    pub detail: String,
}

impl fmt::Display for SemanticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for SemanticError {}

pub fn analyze_program(exprs: &[Expr]) -> Result<(), SemanticError> {
    exprs.iter().try_for_each(analyze_expr)
}

pub fn analyze_expr(expr: &Expr) -> Result<(), SemanticError> {
    let Expr::List(items) = expr else {
        return Ok(());
    };
    let Some(Expr::Symbol(head)) = items.first() else {
        return items.iter().try_for_each(analyze_expr);
    };

    match head.as_str() {
        // Quoted data has no binding or execution semantics to analyze.
        "quote" => Ok(()),
        "lambda" if items.len() >= 3 => analyze_lambda(&items[1], &items[2..]),
        _ => items.iter().skip(1).try_for_each(analyze_expr),
    }
}

fn analyze_lambda(params: &Expr, body: &[Expr]) -> Result<(), SemanticError> {
    if body.len() > 1 {
        return Err(SemanticError {
            kind: SemanticErrorKind::UnsupportedSequentialBody,
            detail: format!(
                "lambda has {} body expressions; CML IR currently represents exactly one",
                body.len()
            ),
        });
    }

    let mut names = Vec::new();
    match params {
        Expr::List(fixed) => collect_parameter_names(fixed, &mut names),
        Expr::DottedList(fixed, rest) => {
            collect_parameter_names(fixed, &mut names);
            if let Expr::Symbol(name) = &**rest {
                names.push(name);
            }
        }
        Expr::Symbol(name) => names.push(name),
        _ => {}
    }

    let mut seen = HashSet::new();
    for name in names {
        // CML's current IR/backend symbol representation is uppercase.  Two
        // source names that collide after that normalization cannot be
        // compiled faithfully, even if their original spelling differs.
        let normalized = name.to_uppercase();
        if !seen.insert(normalized.clone()) {
            return Err(SemanticError {
                kind: SemanticErrorKind::DuplicateParameter,
                detail: format!("lambda parameter `{normalized}` is bound more than once"),
            });
        }
    }

    body.iter().try_for_each(analyze_expr)
}

fn collect_parameter_names<'a>(params: &'a [Expr], names: &mut Vec<&'a str>) {
    for param in params {
        if let Expr::Symbol(name) = param {
            names.push(name);
        }
    }
}
