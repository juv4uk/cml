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

use crate::ast::Expr;
use crate::ir::{Ir, Params, PrimOp, Quoted};

pub fn lower_program(exprs: &[Expr]) -> Result<Vec<Ir>, String> {
    exprs.iter().map(lower_expr).collect()
}

pub fn lower_expr(expr: &Expr) -> Result<Ir, String> {
    match expr {
        Expr::Integer(n) => Ok(Ir::Int(*n)),
        // compiler.rs's compile_expr emits a direct LOADSYM literal for a
        // source string (compatibility.my's `representational-
        // substitutions`), never a variable lookup -- Quote(Sym(..))
        // lowers to exactly that same LOADSYM via compile_quoted.
        Expr::String(s) => Ok(Ir::Quote(Quoted::Sym(s.to_uppercase()))),
        Expr::Symbol(s) => lower_symbol(s),
        Expr::List(list) => lower_list(list),
        Expr::DottedList(_, _) => {
            Err("unquoted dotted list unsupported (matches compiler.rs's compile_expr)".to_string())
        }
    }
}

fn lower_symbol(s: &str) -> Result<Ir, String> {
    match s.to_uppercase().as_str() {
        "T" => Ok(Ir::True),
        "NIL" => Ok(Ir::Nil),
        _ => Ok(Ir::Var(s.to_uppercase())),
    }
}

fn lower_list(list: &[Expr]) -> Result<Ir, String> {
    if list.is_empty() {
        return Ok(Ir::Nil);
    }
    if let Expr::Symbol(func) = &list[0] {
        lower_call(func, &list[1..])
    } else {
        lower_generic_call(&list[0], &list[1..])
    }
}

fn lower_call(func: &str, args: &[Expr]) -> Result<Ir, String> {
    match func {
        "quote" if args.len() == 1 => Ok(Ir::Quote(lower_quoted(&args[0])?)),
        "cond" => lower_cond(args),
        "lambda" if args.len() >= 2 => lower_lambda(args),
        "let" if args.len() == 2 => lower_let(args),
        "def" if args.len() == 2 => lower_def(args),
        "cons" if args.len() == 2 => lower_prim(PrimOp::Cons, args),
        "car" if args.len() == 1 => lower_prim(PrimOp::Car, args),
        "cdr" if args.len() == 1 => lower_prim(PrimOp::Cdr, args),
        "eq" if args.len() == 2 => lower_prim(PrimOp::Eq, args),
        "atom" if args.len() == 1 => lower_prim(PrimOp::Atom, args),
        "equal?" if args.len() == 2 => lower_prim(PrimOp::EqualP, args),
        "+" if args.len() == 2 => lower_prim(PrimOp::Add, args),
        _ => lower_generic_call(&Expr::Symbol(func.to_string()), args),
    }
}

fn lower_prim(op: PrimOp, args: &[Expr]) -> Result<Ir, String> {
    Ok(Ir::Prim { op, args: args.iter().map(lower_expr).collect::<Result<_, _>>()? })
}

fn lower_generic_call(func_expr: &Expr, args: &[Expr]) -> Result<Ir, String> {
    Ok(Ir::App {
        func: Box::new(lower_expr(func_expr)?),
        args: args.iter().map(lower_expr).collect::<Result<_, _>>()?,
    })
}

fn lower_cond(branches: &[Expr]) -> Result<Ir, String> {
    let mut lowered = Vec::with_capacity(branches.len());
    for branch in branches {
        let Expr::List(pair) = branch else {
            return Err("malformed cond branch (matches compiler.rs's compile_cond)".to_string());
        };
        let [test, body] = pair.as_slice() else {
            return Err("malformed cond branch (matches compiler.rs's compile_cond)".to_string());
        };
        lowered.push((lower_expr(test)?, lower_expr(body)?));
    }
    Ok(Ir::Cond { branches: lowered })
}

fn lower_lambda(args: &[Expr]) -> Result<Ir, String> {
    let params = lower_params(&args[0])?;
    let body = lower_expr(&args[1])?;
    Ok(Ir::Lambda { params, body: Box::new(body) })
}

fn lower_params(expr: &Expr) -> Result<Params, String> {
    match expr {
        Expr::List(params) => Ok(Params::Fixed(symbols(params)?)),
        Expr::DottedList(list, tail) => {
            let Expr::Symbol(rest) = &**tail else {
                return Err("dotted param list's tail must be a symbol".to_string());
            };
            Ok(Params::Variadic { fixed: symbols(list)?, rest: rest.to_uppercase() })
        }
        Expr::Symbol(rest) => Ok(Params::AllRest(rest.to_uppercase())),
        _ => Err("malformed lambda parameter list".to_string()),
    }
}

fn symbols(exprs: &[Expr]) -> Result<Vec<String>, String> {
    exprs
        .iter()
        .map(|e| match e {
            Expr::Symbol(s) => Ok(s.to_uppercase()),
            _ => Err("expected a symbol in parameter list".to_string()),
        })
        .collect()
}

fn lower_let(args: &[Expr]) -> Result<Ir, String> {
    let Expr::List(bindings) = &args[0] else {
        return Err("malformed let (matches compiler.rs's compile_let)".to_string());
    };
    let mut lowered_bindings = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let Expr::List(pair) = binding else {
            return Err("malformed let binding".to_string());
        };
        let [Expr::Symbol(name), value] = pair.as_slice() else {
            return Err("malformed let binding".to_string());
        };
        lowered_bindings.push((name.to_uppercase(), lower_expr(value)?));
    }
    let body = lower_expr(&args[1])?;
    Ok(Ir::Let { bindings: lowered_bindings, body: Box::new(body) })
}

fn lower_def(args: &[Expr]) -> Result<Ir, String> {
    let Expr::Symbol(name) = &args[0] else {
        return Err("def expects a symbol name (matches compiler.rs's compile_def)".to_string());
    };
    let value = lower_expr(&args[1])?;
    Ok(Ir::Def { name: name.to_uppercase(), value: Box::new(value) })
}

fn lower_quoted(expr: &Expr) -> Result<Quoted, String> {
    match expr {
        Expr::Integer(n) => Ok(Quoted::Int(*n)),
        Expr::String(s) => Ok(Quoted::Str(s.to_uppercase())),
        Expr::Symbol(s) => Ok(Quoted::Sym(s.to_uppercase())),
        Expr::List(list) => {
            if list.is_empty() {
                Ok(Quoted::Nil)
            } else {
                Ok(Quoted::List(list.iter().map(lower_quoted).collect::<Result<_, _>>()?))
            }
        }
        Expr::DottedList(list, tail) => Ok(Quoted::DottedList(
            list.iter().map(lower_quoted).collect::<Result<_, _>>()?,
            Box::new(lower_quoted(tail)?),
        )),
    }
}
