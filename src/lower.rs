//! `ast::Expr -> ir::Ir` lowering: proves the IR is well-defined for every
//! form `compiler.rs` already supports (docs/heterogeneous-backends.md
//! step 1). Mirrors `compiler.rs`'s dispatch structure (`compile_expr`,
//! `compile_call`, `compile_quote`, `compile_cond`, `compile_lambda`,
//! `compile_let`, `compile_def`) form-for-form, on purpose -- this is a
//! reflection of the existing compiler's actual coverage, not a
//! reimplementation of its semantics.
//!
//! Assumes macro-expanded input, same contract `compiler.rs` has (see
//! `main.rs`: `MacroExpander::new().process(&exprs)` runs first).

use crate::ast::{Expr, NumericBufferLiteral};
use crate::ir::{BufferLiteral, Ir, Params, PrimOp, Quoted};
use crate::semantic::{self, SemanticError};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowerErrorKind {
    Arity,
    InvalidForm,
    Semantic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerError {
    pub kind: LowerErrorKind,
    pub detail: String,
}

impl LowerError {
    fn arity(detail: impl Into<String>) -> Self {
        Self {
            kind: LowerErrorKind::Arity,
            detail: detail.into(),
        }
    }

    fn invalid_form(detail: impl Into<String>) -> Self {
        Self {
            kind: LowerErrorKind::InvalidForm,
            detail: detail.into(),
        }
    }

    fn semantic(error: SemanticError) -> Self {
        Self {
            kind: LowerErrorKind::Semantic,
            detail: error.to_string(),
        }
    }
}

impl fmt::Display for LowerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

pub fn lower_program(exprs: &[Expr]) -> Result<Vec<Ir>, LowerError> {
    let folded: Vec<Expr> = exprs
        .iter()
        .map(crate::pratyahara::fold_constants)
        .collect();
    semantic::analyze_program(&folded).map_err(LowerError::semantic)?;
    folded.iter().map(lower_expr).collect()
}

/// Contract-2.1 lowering for backends that represent builtins as ordinary
/// callable values in the lexical environment.  The shared structural IR
/// still records primitive calls as `Ir::Prim` first (the fpga-lisp 2.0
/// backend depends on that form); this backend-facing pass reifies every
/// primitive use into `App(Var(...), ...)`, recursively, so a local binding
/// can shadow `+`, `car`, etc. through normal environment lookup.
pub fn lower_program_with_first_class_builtins(exprs: &[Expr]) -> Result<Vec<Ir>, LowerError> {
    lower_program(exprs).map(|program| program.into_iter().map(reify_primitive_calls).collect())
}

/// Lower and mark explicit self-tail-calls inside `Def` bodies.
///
/// For each `Def { name, value: Lambda { body } }`, any `App { func:
/// Var(name), args }` that appears in *tail position* inside the body is
/// converted to `TailSelfCall { args }`. The x86 freestanding backend uses
/// this to emit a `jmp` to the function loop-entry instead of a native `call`,
/// keeping the stack depth bounded regardless of recursion depth.
///
/// Only direct self-calls are transformed; mutual recursion, closures and
/// general function calls remain `Ir::App` and are rejected by the x86
/// backend's preflight gate.
pub fn lower_program_with_tail_calls(exprs: &[Expr]) -> Result<Vec<Ir>, LowerError> {
    lower_program(exprs).map(|program| {
        program
            .into_iter()
            .map(|node| match node {
                Ir::Def {
                    ref name,
                    value: ref lambda,
                } => {
                    if let Ir::Lambda { ref params, ref body } = **lambda {
                        let new_body = mark_tail_position(body, name);
                        Ir::Def {
                            name: name.clone(),
                            value: Box::new(Ir::Lambda {
                                params: params.clone(),
                                body: Box::new(new_body),
                            }),
                        }
                    } else {
                        node
                    }
                }
                other => other,
            })
            .collect()
    })
}

/// Rewrite `App { func: Var(self_name), args }` → `TailSelfCall { args }`
/// whenever it appears in tail position within `ir`.
///
/// "Tail position" here means: the node is the last value-producing
/// expression — in particular, it is the result of a `Cond` branch, a `Let`
/// body, or the lambda body itself.  Sub-expressions of `Prim` and `App`
/// argument lists are *not* in tail position.
fn mark_tail_position(ir: &Ir, self_name: &str) -> Ir {
    match ir {
        // Direct self-call in tail position.
        Ir::App { func, args } => {
            if let Ir::Var(name) = func.as_ref() {
                if name == self_name {
                    return Ir::TailSelfCall {
                        args: args.clone(),
                    };
                }
            }
            ir.clone()
        }
        // Tail position propagates through the branches of Cond.
        Ir::Cond { branches } => Ir::Cond {
            branches: branches
                .iter()
                .map(|(test, body)| (test.clone(), mark_tail_position(body, self_name)))
                .collect(),
        },
        // Tail position propagates through the body of Let.
        Ir::Let { bindings, body } => Ir::Let {
            bindings: bindings.clone(),
            body: Box::new(mark_tail_position(body, self_name)),
        },
        // All other nodes are leaves or non-tail contexts — clone unchanged.
        other => other.clone(),
    }
}

fn reify_primitive_calls(ir: Ir) -> Ir {
    match ir {
        Ir::Prim { op, args } => Ir::App {
            func: Box::new(Ir::Var(primitive_name(op).to_string())),
            args: args.into_iter().map(reify_primitive_calls).collect(),
        },
        Ir::Lambda { params, body } => Ir::Lambda {
            params,
            body: Box::new(reify_primitive_calls(*body)),
        },
        Ir::App { func, args } => Ir::App {
            func: Box::new(reify_primitive_calls(*func)),
            args: args.into_iter().map(reify_primitive_calls).collect(),
        },
        Ir::Cond { branches } => Ir::Cond {
            branches: branches
                .into_iter()
                .map(|(test, body)| (reify_primitive_calls(test), reify_primitive_calls(body)))
                .collect(),
        },
        Ir::Let { bindings, body } => Ir::Let {
            bindings: bindings
                .into_iter()
                .map(|(name, value)| (name, reify_primitive_calls(value)))
                .collect(),
            body: Box::new(reify_primitive_calls(*body)),
        },
        Ir::Def { name, value } => Ir::Def {
            name,
            value: Box::new(reify_primitive_calls(*value)),
        },
        leaf => leaf,
    }
}

fn primitive_name(op: PrimOp) -> &'static str {
    match op {
        PrimOp::Add => "+",
        PrimOp::Sub => "-",
        PrimOp::Cons => "CONS",
        PrimOp::Car => "CAR",
        PrimOp::Cdr => "CDR",
        PrimOp::Eq => "EQ",
        PrimOp::Atom => "ATOM",
        PrimOp::EqualP => "EQUAL?",
    }
}

pub fn lower_expr(expr: &Expr) -> Result<Ir, LowerError> {
    semantic::analyze_expr(expr).map_err(LowerError::semantic)?;
    lower_expr_admitted(expr)
}

fn lower_expr_admitted(expr: &Expr) -> Result<Ir, LowerError> {
    match expr {
        Expr::Integer(n) => Ok(Ir::Int(*n)),
        Expr::NumericBuffer(NumericBufferLiteral::I32(values)) => {
            Ok(Ir::Buffer(BufferLiteral::I32(values.clone())))
        }
        Expr::NumericBuffer(NumericBufferLiteral::F32(values)) => {
            Ok(Ir::Buffer(BufferLiteral::F32(values.clone())))
        }
        // compiler.rs's compile_expr emits a direct LOADSYM literal for a
        // source string (compatibility.my's `representational-
        // substitutions`), never a variable lookup -- Quote(Sym(..))
        // lowers to exactly that same LOADSYM via compile_quoted.
        Expr::String(s) => Ok(Ir::Quote(Quoted::Sym(s.to_uppercase()))),
        Expr::Symbol(s) => lower_symbol(s),
        Expr::List(list) => lower_list(list),
        Expr::DottedList(_, _) => Err(LowerError::invalid_form(
            "unquoted dotted list unsupported (matches compiler.rs's compile_expr)",
        )),
    }
}

fn lower_symbol(s: &str) -> Result<Ir, LowerError> {
    match s.to_uppercase().as_str() {
        "T" => Ok(Ir::True),
        "NIL" => Ok(Ir::Nil),
        _ => Ok(Ir::Var(s.to_uppercase())),
    }
}

fn lower_list(list: &[Expr]) -> Result<Ir, LowerError> {
    if list.is_empty() {
        return Ok(Ir::Nil);
    }
    if let Expr::Symbol(func) = &list[0] {
        lower_call(func, &list[1..])
    } else {
        lower_generic_call(&list[0], &list[1..])
    }
}

fn lower_call(func: &str, args: &[Expr]) -> Result<Ir, LowerError> {
    match func {
        "quote" if args.len() == 1 => Ok(Ir::Quote(lower_quoted(&args[0])?)),
        "quote" => Err(LowerError::arity("quote expects exactly one argument")),
        "cond" => lower_cond(args),
        "lambda" if args.len() >= 2 => lower_lambda(args),
        "let" if args.len() == 2 => lower_let(args),
        "def" if args.len() == 2 => lower_def(args),
        "numeric-buffer-map" if args.len() == 2 => {
            lower_generic_call(&Expr::Symbol("NUMERIC-BUFFER-MAP".to_string()), args)
        }
        "numeric-buffer-map" => Err(LowerError::arity(
            "numeric-buffer-map expects exactly two arguments",
        )),
        "cons" if args.len() == 2 => lower_prim(PrimOp::Cons, args),
        "car" if args.len() == 1 => lower_prim(PrimOp::Car, args),
        "cdr" if args.len() == 1 => lower_prim(PrimOp::Cdr, args),
        "eq" if args.len() == 2 => lower_prim(PrimOp::Eq, args),
        "atom" if args.len() == 1 => lower_prim(PrimOp::Atom, args),
        "equal?" if args.len() == 2 => lower_prim(PrimOp::EqualP, args),
        "+" if args.len() == 2 => lower_prim(PrimOp::Add, args),
        "-" if args.len() == 2 => lower_prim(PrimOp::Sub, args),
        _ => lower_generic_call(&Expr::Symbol(func.to_string()), args),
    }
}

fn lower_prim(op: PrimOp, args: &[Expr]) -> Result<Ir, LowerError> {
    Ok(Ir::Prim {
        op,
        args: args.iter().map(lower_expr).collect::<Result<_, _>>()?,
    })
}

fn lower_generic_call(func_expr: &Expr, args: &[Expr]) -> Result<Ir, LowerError> {
    Ok(Ir::App {
        func: Box::new(lower_expr(func_expr)?),
        args: args.iter().map(lower_expr).collect::<Result<_, _>>()?,
    })
}

fn lower_cond(branches: &[Expr]) -> Result<Ir, LowerError> {
    let mut lowered = Vec::with_capacity(branches.len());
    for branch in branches {
        let Expr::List(pair) = branch else {
            return Err(LowerError::invalid_form(
                "malformed cond branch (matches compiler.rs's compile_cond)",
            ));
        };
        let [test, body] = pair.as_slice() else {
            return Err(LowerError::invalid_form(
                "malformed cond branch (matches compiler.rs's compile_cond)",
            ));
        };
        lowered.push((lower_expr(test)?, lower_expr(body)?));
    }
    Ok(Ir::Cond { branches: lowered })
}

fn lower_lambda(args: &[Expr]) -> Result<Ir, LowerError> {
    let params = lower_params(&args[0])?;
    let body = lower_expr(&args[1])?;
    Ok(Ir::Lambda {
        params,
        body: Box::new(body),
    })
}

fn lower_params(expr: &Expr) -> Result<Params, LowerError> {
    match expr {
        Expr::List(params) => Ok(Params::Fixed(symbols(params)?)),
        Expr::DottedList(list, tail) => {
            let Expr::Symbol(rest) = &**tail else {
                return Err(LowerError::invalid_form(
                    "dotted param list's tail must be a symbol",
                ));
            };
            Ok(Params::Variadic {
                fixed: symbols(list)?,
                rest: rest.to_uppercase(),
            })
        }
        Expr::Symbol(rest) => Ok(Params::AllRest(rest.to_uppercase())),
        _ => Err(LowerError::invalid_form("malformed lambda parameter list")),
    }
}

fn symbols(exprs: &[Expr]) -> Result<Vec<String>, LowerError> {
    exprs
        .iter()
        .map(|e| match e {
            Expr::Symbol(s) => Ok(s.to_uppercase()),
            _ => Err(LowerError::invalid_form(
                "expected a symbol in parameter list",
            )),
        })
        .collect()
}

fn lower_let(args: &[Expr]) -> Result<Ir, LowerError> {
    let Expr::List(bindings) = &args[0] else {
        return Err(LowerError::invalid_form(
            "malformed let (matches compiler.rs's compile_let)",
        ));
    };
    let mut lowered_bindings = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let Expr::List(pair) = binding else {
            return Err(LowerError::invalid_form("malformed let binding"));
        };
        let [Expr::Symbol(name), value] = pair.as_slice() else {
            return Err(LowerError::invalid_form("malformed let binding"));
        };
        lowered_bindings.push((name.to_uppercase(), lower_expr(value)?));
    }
    let body = lower_expr(&args[1])?;
    Ok(Ir::Let {
        bindings: lowered_bindings,
        body: Box::new(body),
    })
}

fn lower_def(args: &[Expr]) -> Result<Ir, LowerError> {
    let Expr::Symbol(name) = &args[0] else {
        return Err(LowerError::invalid_form(
            "def expects a symbol name (matches compiler.rs's compile_def)",
        ));
    };
    let value = lower_expr(&args[1])?;
    Ok(Ir::Def {
        name: name.to_uppercase(),
        value: Box::new(value),
    })
}

fn lower_quoted(expr: &Expr) -> Result<Quoted, LowerError> {
    match expr {
        Expr::Integer(n) => Ok(Quoted::Int(*n)),
        Expr::String(s) => Ok(Quoted::Str(s.to_uppercase())),
        Expr::Symbol(s) => Ok(Quoted::Sym(s.to_uppercase())),
        Expr::List(list) => {
            if list.is_empty() {
                Ok(Quoted::Nil)
            } else {
                Ok(Quoted::List(
                    list.iter().map(lower_quoted).collect::<Result<_, _>>()?,
                ))
            }
        }
        Expr::DottedList(list, tail) => Ok(Quoted::DottedList(
            list.iter().map(lower_quoted).collect::<Result<_, _>>()?,
            Box::new(lower_quoted(tail)?),
        )),
        Expr::NumericBuffer(_) => Err(LowerError::invalid_form(
            "numeric buffers are self-evaluating values and must not be quoted",
        )),
    }
}
