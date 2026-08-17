// Macro expansion: a compile-time-only pass, run before `Compiler::compile`.
// `defmacro` bodies are evaluated over unevaluated argument ASTs using a
// tiny meta-evaluator (quote/cons/car/cdr/atom/eq/cond), then the resulting
// Expr replaces the call site and is expanded again (for nested macros)
// before ever reaching the FPGA compiler.
// Розгортання макросів — прохід лише на етапі компіляції, до Compiler::compile.
// Erweiterung von Makros — ein reiner Kompilierzeit-Durchlauf vor Compiler::compile.

use crate::ast::Expr;
use std::collections::HashMap;
use std::fmt;

/// Errors that can occur during macro expansion.
#[derive(Debug, Clone)]
pub enum MacroError {
    /// An unbound symbol was referenced in a macro body.
    UnboundSymbol(String),
    /// A list form in a macro body didn't start with a symbol operator.
    ExpectedOperator,
    /// `car` was applied to a non-pair expression.
    CarOfNonPair,
    /// `cdr` was applied to a non-pair expression.
    CdrOfNonPair,
    /// An unsupported form was encountered in a macro body.
    UnsupportedForm(String),
}

impl fmt::Display for MacroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MacroError::UnboundSymbol(s) => write!(f, "macro body: unbound symbol `{s}`"),
            MacroError::ExpectedOperator => write!(f, "macro body: expected operator symbol"),
            MacroError::CarOfNonPair => write!(f, "macro body: car of non-pair"),
            MacroError::CdrOfNonPair => write!(f, "macro body: cdr of non-pair"),
            MacroError::UnsupportedForm(form) => write!(f, "macro body: unsupported form `{form}`"),
        }
    }
}

impl std::error::Error for MacroError {}

struct MacroDef {
    params: Expr,
    body: Expr,
}

pub struct MacroExpander {
    macros: HashMap<String, MacroDef>,
}

fn is_nil(expr: &Expr) -> bool {
    matches!(expr, Expr::List(l) if l.is_empty())
}

fn nil() -> Expr {
    Expr::List(Vec::new())
}

fn truthy(cond: bool) -> Expr {
    if cond {
        Expr::Symbol("T".to_string())
    } else {
        nil()
    }
}

fn cons_expr(head: Expr, tail: Expr) -> Expr {
    match tail {
        Expr::List(mut list) => {
            list.insert(0, head);
            Expr::List(list)
        }
        Expr::DottedList(mut list, dtail) => {
            list.insert(0, head);
            Expr::DottedList(list, dtail)
        }
        other => Expr::DottedList(vec![head], Box::new(other)),
    }
}

fn split(expr: &Expr) -> Option<(Expr, Expr)> {
    match expr {
        Expr::List(list) if !list.is_empty() => {
            let head = list[0].clone();
            let tail = Expr::List(list[1..].to_vec());
            Some((head, tail))
        }
        Expr::DottedList(list, dtail) if !list.is_empty() => {
            let head = list[0].clone();
            let tail = if list.len() == 1 {
                (**dtail).clone()
            } else {
                Expr::DottedList(list[1..].to_vec(), dtail.clone())
            };
            Some((head, tail))
        }
        _ => None,
    }
}

impl MacroExpander {
    pub fn new() -> Self {
        MacroExpander { macros: HashMap::new() }
    }

    pub fn process(&mut self, exprs: &[Expr]) -> Result<Vec<Expr>, MacroError> {
        let mut out = Vec::new();
        for expr in exprs {
            if let Some((name, params, body)) = as_defmacro(expr) {
                self.macros.insert(name, MacroDef { params, body });
            } else {
                out.push(self.expand(expr)?);
            }
        }
        Ok(out)
    }

    fn expand(&self, expr: &Expr) -> Result<Expr, MacroError> {
        match expr {
            Expr::List(list) if !list.is_empty() => {
                if let Expr::Symbol(name) = &list[0] {
                    if name == "quote" {
                        return Ok(expr.clone());
                    }
                    if let Some(mac) = self.macros.get(name) {
                        let bindings = bind_params(&mac.params, &list[1..]);
                        let expanded_value = eval_macro_body(&mac.body, &bindings)?;
                        return self.expand(&expanded_value);
                    }
                }
                Ok(Expr::List(list.iter().map(|e| self.expand(e)).collect::<Result<Vec<_>, _>>()?))
            }
            _ => Ok(expr.clone()),
        }
    }
}

fn as_defmacro(expr: &Expr) -> Option<(String, Expr, Expr)> {
    let Expr::List(list) = expr else { return None };
    let [Expr::Symbol(kw), Expr::Symbol(name), params, body] = list.as_slice() else {
        return None;
    };
    if kw != "defmacro" {
        return None;
    }
    Some((name.clone(), params.clone(), body.clone()))
}

fn bind_params(params: &Expr, args: &[Expr]) -> HashMap<String, Expr> {
    let mut env = HashMap::new();
    match params {
        Expr::Symbol(name) => {
            env.insert(name.clone(), Expr::List(args.to_vec()));
        }
        Expr::List(names) => {
            for (name, arg) in names.iter().zip(args.iter()) {
                if let Expr::Symbol(n) = name {
                    env.insert(n.clone(), arg.clone());
                }
            }
        }
        Expr::DottedList(names, tail) => {
            for (name, arg) in names.iter().zip(args.iter()) {
                if let Expr::Symbol(n) = name {
                    env.insert(n.clone(), arg.clone());
                }
            }
            if let Expr::Symbol(tail_name) = &**tail {
                let rest = args.iter().skip(names.len()).cloned().collect();
                env.insert(tail_name.clone(), Expr::List(rest));
            }
        }
        _ => {}
    }
    env
}

fn eval_macro_body(expr: &Expr, env: &HashMap<String, Expr>) -> Result<Expr, MacroError> {
    match expr {
        Expr::Integer(_) | Expr::String(_) => Ok(expr.clone()),
        Expr::Symbol(s) => {
            let upper = s.to_uppercase();
            if upper == "NIL" {
                Ok(nil())
            } else if upper == "T" {
                Ok(expr.clone())
            } else {
                Ok(env.get(s)
                    .ok_or_else(|| MacroError::UnboundSymbol(s.clone()))?
                    .clone())
            }
        }
        Expr::DottedList(_, _) => Ok(expr.clone()),
        Expr::List(list) => {
            if list.is_empty() {
                return Ok(nil());
            }
            let Expr::Symbol(op) = &list[0] else {
                return Err(MacroError::ExpectedOperator);
            };
            match op.as_str() {
                "quote" => Ok(list[1].clone()),
                "cons" => {
                    let head = eval_macro_body(&list[1], env)?;
                    let tail = eval_macro_body(&list[2], env)?;
                    Ok(cons_expr(head, tail))
                }
                "car" => split(&eval_macro_body(&list[1], env)?)
                    .map(|(h, _)| h)
                    .ok_or(MacroError::CarOfNonPair),
                "cdr" => split(&eval_macro_body(&list[1], env)?)
                    .map(|(_, t)| t)
                    .ok_or(MacroError::CdrOfNonPair),
                "atom" => Ok(truthy(split(&eval_macro_body(&list[1], env)?).is_none())),
                "eq" => {
                    let a = eval_macro_body(&list[1], env)?;
                    let b = eval_macro_body(&list[2], env)?;
                    Ok(truthy(a == b))
                }
                "cond" => {
                    for branch in &list[1..] {
                        let Expr::List(pair) = branch else { continue };
                        if pair.len() != 2 {
                            continue;
                        }
                        let test = eval_macro_body(&pair[0], env)?;
                        if !is_nil(&test) {
                            return eval_macro_body(&pair[1], env);
                        }
                    }
                    Ok(nil())
                }
                other => Err(MacroError::UnsupportedForm(other.to_string())),
            }
        }
    }
}
