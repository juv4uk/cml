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
                    if let Ir::Lambda {
                        ref params,
                        ref body,
                    } = **lambda
                    {
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
                    return Ir::TailSelfCall { args: args.clone() };
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
        Ir::Builtin(name) => Ir::Var(name),
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

#[derive(Clone, Default)]
struct Env {
    bound: Vec<String>,
}

impl Env {
    fn is_bound(&self, name: &str) -> bool {
        self.bound.iter().any(|b| b == name)
    }
}

pub fn lower_expr(expr: &Expr) -> Result<Ir, LowerError> {
    semantic::analyze_expr(expr).map_err(LowerError::semantic)?;
    lower_expr_admitted(expr, &Env::default())
}

fn lower_expr_admitted(expr: &Expr, env: &Env) -> Result<Ir, LowerError> {
    match expr {
        Expr::Integer(n) => Ok(Ir::Int(*n)),
        Expr::Rational(num, den) => Ok(Ir::Rational(*num, *den)),
        Expr::NumericBuffer(NumericBufferLiteral::I32(values)) => {
            Ok(Ir::Buffer(BufferLiteral::I32(values.clone())))
        }
        Expr::NumericBuffer(NumericBufferLiteral::F32(values)) => {
            Ok(Ir::Buffer(BufferLiteral::F32(values.clone())))
        }
        Expr::String(s) => Ok(Ir::String(s.clone())),
        Expr::Symbol(s) => lower_symbol(s, env),
        Expr::List(list) => lower_list(list, env),
        Expr::DottedList(_, _) => Err(LowerError::invalid_form(
            "unquoted dotted list unsupported (matches compiler.rs's compile_expr)",
        )),
    }
}

/// cml#9 Finding 1: true if `s` is a special-form spelling in ANY
/// Canon-registered language (quote/cond/lambda/define/defmacro), not only
/// the hardcoded English one. `let` has no Canon registry entry -- it is a
/// cml-only construct -- so it stays English-only on purpose.
fn is_special_form_symbol(s: &str, upper: &str) -> bool {
    use crate::canon::{
        CANON_COND_EXACT, CANON_COND_UPPER, CANON_DEFINE_EXACT, CANON_DEFINE_UPPER,
        CANON_DEFMACRO_EXACT, CANON_DEFMACRO_UPPER, CANON_LAMBDA_EXACT, CANON_LAMBDA_UPPER,
        CANON_QUOTE_EXACT, CANON_QUOTE_UPPER, is_canon_form,
    };
    upper == "LET"
        || is_canon_form(s, CANON_QUOTE_UPPER, CANON_QUOTE_EXACT)
        || is_canon_form(s, CANON_COND_UPPER, CANON_COND_EXACT)
        || is_canon_form(s, CANON_LAMBDA_UPPER, CANON_LAMBDA_EXACT)
        || is_canon_form(s, CANON_DEFINE_UPPER, CANON_DEFINE_EXACT)
        || is_canon_form(s, CANON_DEFMACRO_UPPER, CANON_DEFMACRO_EXACT)
}

fn lower_symbol(s: &str, env: &Env) -> Result<Ir, LowerError> {
    let upper = s.to_uppercase();
    if is_special_form_symbol(s, &upper) {
        return if env.is_bound(&upper) {
            Ok(Ir::Var(upper))
        } else {
            Err(LowerError::invalid_form(
                "special forms are not callable values",
            ))
        };
    }

    // Canon callable meaning comes from the semantic registry even when the
    // function appears as a first-class value rather than in call position.
    // Rust only projects the opaque semantic ID onto the compiler mechanism.
    if !env.is_bound(&upper) {
        if let Some(semantic_id) = crate::canon::callable_semantic_id(s) {
            let builtin_name = match semantic_id {
                "0002" => Some("ATOM"),
                "0003" => Some("EQ"),
                "0004" => Some("CONS"),
                "0005" => Some("CAR"),
                "0006" => Some("CDR"),
                _ => None,
            };
            if let Some(name) = builtin_name {
                return Ok(Ir::Builtin(name.to_string()));
            }
        }
    }

    match upper.as_str() {
        // `t`/`nil` are not in the Canon 0+7 reserved set (semantic.rs) --
        // my-lisp treats them as ordinary lexical bindings, shadowable the
        // same way `car`/`cons`/etc already are below. A lexical binding
        // named `t` or `nil` (e.g. a lambda parameter) must resolve to that
        // binding, not silently collapse into the literal, or a real
        // program using `t`/`nil` as an ordinary name would observe wrong
        // values with no error at all.
        "T" if env.is_bound(&upper) => Ok(Ir::Var(upper)),
        "T" => Ok(Ir::True),
        "NIL" if env.is_bound(&upper) => Ok(Ir::Var(upper)),
        "NIL" => Ok(Ir::Nil),
        "CONS" | "CAR" | "CDR" | "EQ" | "ATOM" | "EQUAL?" | "+" | "-" | "NUMERIC-BUFFER-MAP" => {
            if env.is_bound(&upper) {
                Ok(Ir::Var(upper))
            } else {
                Ok(Ir::Builtin(upper))
            }
        }
        _ => Ok(Ir::Var(upper)),
    }
}

fn lower_list(list: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    if list.is_empty() {
        return Ok(Ir::Nil);
    }
    if let Expr::Symbol(func) = &list[0] {
        lower_call(func, &list[1..], env)
    } else {
        lower_generic_call(&list[0], &list[1..], env)
    }
}

fn lower_call(func: &str, args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    use crate::canon::{
        CANON_COND_EXACT, CANON_COND_UPPER, CANON_DEFINE_EXACT, CANON_DEFINE_UPPER,
        CANON_LAMBDA_EXACT, CANON_LAMBDA_UPPER, CANON_QUOTE_EXACT, CANON_QUOTE_UPPER,
        callable_semantic_id, is_canon_form,
    };
    // cml#9 Finding 1: dispatch on the Canon identity of `func` (any
    // registered language), not only its English spelling.
    if is_canon_form(func, CANON_QUOTE_UPPER, CANON_QUOTE_EXACT) {
        return match args {
            [single] => Ok(Ir::Quote(lower_quoted(single)?)),
            _ => Err(LowerError::arity("quote expects exactly one argument")),
        };
    }
    if is_canon_form(func, CANON_COND_UPPER, CANON_COND_EXACT) {
        return lower_cond(args, env);
    }
    if is_canon_form(func, CANON_LAMBDA_UPPER, CANON_LAMBDA_EXACT) && args.len() >= 2 {
        return lower_lambda(args, env);
    }
    if func == "let" && args.len() == 2 {
        return lower_let(args, env);
    }
    if is_canon_form(func, CANON_DEFINE_UPPER, CANON_DEFINE_EXACT) && args.len() == 2 {
        return lower_def(args, env);
    }

    let upper = func.to_uppercase();
    if !env.is_bound(&upper) {
        // Canon callable identity comes from my-lisp's registry. The match
        // below is only the compiler's finite mechanism projection from an
        // opaque semantic ID to the IR operation it can implement.
        if let Some(semantic_id) = callable_semantic_id(func) {
            match (semantic_id, args.len()) {
                ("0002", 1) => return lower_prim(PrimOp::Atom, args, env),
                ("0003", 2) => return lower_prim(PrimOp::Eq, args, env),
                ("0004", 2) => return lower_prim(PrimOp::Cons, args, env),
                ("0005", 1) => return lower_prim(PrimOp::Car, args, env),
                ("0006", 1) => return lower_prim(PrimOp::Cdr, args, env),
                _ => {}
            }
        }

        match func {
            "numeric-buffer-map" if args.len() == 2 => {
                return lower_generic_call(
                    &Expr::Symbol("NUMERIC-BUFFER-MAP".to_string()),
                    args,
                    env,
                );
            }
            "numeric-buffer-map" => {
                return Err(LowerError::arity(
                    "numeric-buffer-map expects exactly two arguments",
                ));
            }
            "equal?" if args.len() == 2 => return lower_prim(PrimOp::EqualP, args, env),
            "+" if args.len() == 2 => return lower_prim(PrimOp::Add, args, env),
            "-" if args.len() == 2 => return lower_prim(PrimOp::Sub, args, env),
            _ => {}
        }
    }

    lower_generic_call(&Expr::Symbol(func.to_string()), args, env)
}

fn lower_prim(op: PrimOp, args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    Ok(Ir::Prim {
        op,
        args: args
            .iter()
            .map(|e| lower_expr_admitted(e, env))
            .collect::<Result<_, _>>()?,
    })
}

fn lower_generic_call(func_expr: &Expr, args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    Ok(Ir::App {
        func: Box::new(lower_expr_admitted(func_expr, env)?),
        args: args
            .iter()
            .map(|e| lower_expr_admitted(e, env))
            .collect::<Result<_, _>>()?,
    })
}

fn lower_cond(branches: &[Expr], env: &Env) -> Result<Ir, LowerError> {
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
        lowered.push((
            lower_expr_admitted(test, env)?,
            lower_expr_admitted(body, env)?,
        ));
    }
    Ok(Ir::Cond { branches: lowered })
}

fn lower_lambda(args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    let params = lower_params(&args[0])?;
    let mut inner = env.clone();
    match &params {
        Params::Fixed(names) => inner.bound.extend_from_slice(names),
        Params::Variadic { fixed, rest } => {
            inner.bound.extend_from_slice(fixed);
            inner.bound.push(rest.clone());
        }
        Params::AllRest(rest) => inner.bound.push(rest.clone()),
    }
    let body = lower_expr_admitted(&args[1], &inner)?;
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

fn lower_let(args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    let Expr::List(bindings) = &args[0] else {
        return Err(LowerError::invalid_form(
            "malformed let (matches compiler.rs's compile_let)",
        ));
    };
    let mut lowered_bindings = Vec::with_capacity(bindings.len());
    let mut inner = env.clone();
    for binding in bindings {
        let Expr::List(pair) = binding else {
            return Err(LowerError::invalid_form("malformed let binding"));
        };
        let [Expr::Symbol(name), value] = pair.as_slice() else {
            return Err(LowerError::invalid_form("malformed let binding"));
        };
        let name_upper = name.to_uppercase();
        lowered_bindings.push((name_upper.clone(), lower_expr_admitted(value, env)?));
        inner.bound.push(name_upper);
    }
    let body = lower_expr_admitted(&args[1], &inner)?;
    Ok(Ir::Let {
        bindings: lowered_bindings,
        body: Box::new(body),
    })
}

fn lower_def(args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    let Expr::Symbol(name) = &args[0] else {
        return Err(LowerError::invalid_form(
            "def expects a symbol name (matches compiler.rs's compile_def)",
        ));
    };
    let value = lower_expr_admitted(&args[1], env)?;
    Ok(Ir::Def {
        name: name.to_uppercase(),
        value: Box::new(value),
    })
}

fn lower_quoted(expr: &Expr) -> Result<Quoted, LowerError> {
    match expr {
        Expr::Integer(n) => Ok(Quoted::Int(*n)),
        Expr::Rational(num, den) => Ok(Quoted::Rational(*num, *den)),
        // Case-preserving: unlike a symbol, a string's character content is
        // user data, not a target identifier -- cml's uppercasing convention
        // is specific to how the target represents symbols (see the
        // unquoted Expr::String -> Ir::String arm above, which already
        // preserves case), and must never touch string content.
        Expr::String(s) => Ok(Quoted::Str(s.clone())),
        Expr::Symbol(s) => Ok(Quoted::Sym {
            uppercased: s.to_uppercase(),
            original: s.clone(),
        }),
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
