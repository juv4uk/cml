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

fn top_level_definition_name(expr: &Expr) -> Option<String> {
    use crate::canon::{CANON_DEFINE_EXACT, CANON_DEFINE_UPPER, is_canon_form};

    let Expr::List(items) = expr else {
        return None;
    };
    let [head, name, _] = items.as_slice() else {
        return None;
    };
    let is_define_head = match head {
        Expr::Symbol(form) => is_canon_form(form, CANON_DEFINE_UPPER, CANON_DEFINE_EXACT),
        Expr::Sid(sid) => *sid == my_lisp::sid!(00001001) || *sid == my_lisp::sid!(00001011),
        _ => false,
    };
    if !is_define_head {
        return None;
    }
    match name {
        Expr::Symbol(s) => Some(s.to_uppercase()),
        // A SID-keyed define (key_definition_by_sid after #239) uses the
        // typed Sid8's own bit pattern as the def's internal key -- no
        // surface/registry lookup.
        Expr::Sid(sid) => Some(sid.to_string()),
        _ => None,
    }
}

/// Returns the SID used as a definition key, if this top-level form is a
/// define whose name is a typed Sid8. Name-keyed surface defines return `None`.
fn sid_keyed_definition_name(expr: &Expr) -> Option<my_lisp::Sid8> {
    use crate::canon::{CANON_DEFINE_EXACT, CANON_DEFINE_UPPER, is_canon_form};

    let Expr::List(items) = expr else {
        return None;
    };
    let [head, name, _] = items.as_slice() else {
        return None;
    };
    let is_define_head = match head {
        Expr::Symbol(form) => is_canon_form(form, CANON_DEFINE_UPPER, CANON_DEFINE_EXACT),
        Expr::Sid(sid) => *sid == my_lisp::sid!(00001001) || *sid == my_lisp::sid!(00001011),
        _ => false,
    };
    if !is_define_head {
        return None;
    }
    match name {
        Expr::Sid(sid) => Some(*sid),
        _ => None,
    }
}

pub fn lower_program(exprs: &[Expr]) -> Result<Vec<Ir>, LowerError> {
    let folded: Vec<Expr> = exprs
        .iter()
        .map(crate::pratyahara::fold_constants)
        .collect();
    semantic::analyze_program(&folded).map_err(LowerError::semantic)?;

    // Contract 2.1: top-level definitions are ordinary bindings too. Mirror
    // the backends' declaration pass so a definition can shadow a registry
    // callable in its own body and in later/peer top-level forms. This is only
    // name resolution; the backend still owns its existing closure/letrec
    // mechanism and may fail closed for an unsupported executable shape.
    let mut sid_keyed_defs = std::collections::HashSet::new();
    for expr in &folded {
        if let Some(sid) = sid_keyed_definition_name(expr) {
            sid_keyed_defs.insert(sid.to_string());
        }
    }
    let mut env = Env {
        bound: Vec::new(),
        sid_keyed_defs,
    };
    for expr in &folded {
        if let Some(name) = top_level_definition_name(expr) {
            if !env.is_bound(&name) {
                env.bound.push(name);
            }
        }
    }

    folded
        .iter()
        .map(|expr| lower_expr_admitted(expr, &env))
        .collect()
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
/// expression — in particular, it is the result of a control branch, a `Let`
/// body, or the lambda body itself. Sub-expressions of `Prim` and `App`
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
        // Tail position propagates through historical two-part Cond bodies.
        Ir::Cond { branches } => Ir::Cond {
            branches: branches
                .iter()
                .map(|(test, body)| (test.clone(), mark_tail_position(body, self_name)))
                .collect(),
        },
        // Canonical three-part control keeps query/expected untouched while
        // propagating tail position into the selected body only.
        Ir::CondMatch { branches } => Ir::CondMatch {
            branches: branches
                .iter()
                .map(|(query, expected, body)| {
                    (
                        query.clone(),
                        expected.clone(),
                        mark_tail_position(body, self_name),
                    )
                })
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
        Ir::MachinePrim { op, args } => Ir::MachinePrim {
            op,
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
        Ir::CondMatch { branches } => Ir::CondMatch {
            branches: branches
                .into_iter()
                .map(|(query, expected, body)| {
                    (
                        reify_primitive_calls(query),
                        expected,
                        reify_primitive_calls(body),
                    )
                })
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
        PrimOp::ExactQLt => "<",
        PrimOp::ExactQLe => "<=",
        PrimOp::ExactQGe => ">=",
        PrimOp::Cddr => "CDDR",
        PrimOp::Cadddr => "CADDDR",
        PrimOp::Cddr => "CDDR",
        PrimOp::Caar => "CAAR",
        PrimOp::Cadr => "CADR",
        PrimOp::Caddr => "CADDR",
        PrimOp::Caar => "CAAR",
        PrimOp::Cadr => "CADR",
        PrimOp::Caddr => "CADDR",
        PrimOp::List => "LIST",
        PrimOp::Quotient => "QUOTIENT",
    }
}

#[derive(Clone, Default)]
struct Env {
    bound: Vec<String>,
    /// Set of definition keys that are typed 8-bit SID patterns (from
    /// byte-SID `define` rows). A name-keyed call is lowered to `App(Sid)`
    /// only when the target def is actually registered under that SID, so
    /// the call identity matches the def identity (#238).
    sid_keyed_defs: std::collections::HashSet<String>,
}

impl Env {
    fn is_bound(&self, name: &str) -> bool {
        self.bound.iter().any(|b| b == name)
    }

    fn has_sid_keyed_def(&self, sid: my_lisp::Sid8) -> bool {
        self.sid_keyed_defs.contains(&sid.to_string())
    }
}

pub fn lower_expr(expr: &Expr) -> Result<Ir, LowerError> {
    semantic::analyze_expr(expr).map_err(LowerError::semantic)?;
    lower_expr_admitted(expr, &Env::default())
}

fn lower_expr_admitted(expr: &Expr, env: &Env) -> Result<Ir, LowerError> {
    match expr {
        Expr::Sid(sid) => Ok(Ir::Sid(*sid)),
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
            if let Some(name) = crate::canon::canonical_builtin_name(semantic_id) {
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
        _ => Ok(Ir::Var(upper)),
    }
}

fn lower_list(list: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    if list.is_empty() {
        return Ok(Ir::Nil);
    }
    if let Expr::Symbol(func) = &list[0] {
        lower_call(func, &list[1..], env)
    } else if let Expr::Sid(sid) = &list[0] {
        lower_sid_head(*sid, &list[1..], env)
    } else {
        lower_generic_call(&list[0], &list[1..], env)
    }
}

/// Typed SID-head routing for the special forms, mirroring `lower_call`'s
/// Canon-surface dispatch so byte-SID-authored source (pinned `core.lisp`
/// rows such as `(00001000 args args)`) lowers through the same mechanism
/// as the surface spellings. Only the special-form SIDs are intercepted
/// here; every other SID remains a first-class call value and keeps
/// `Ir::App { func: Ir::Sid(sid), .. }` for the direct-SID8 dispatch
/// (#238), never a SID-to-name fallback.
fn lower_sid_head(sid: my_lisp::Sid8, args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    if sid == my_lisp::sid!(00000001) {
        return match args {
            [single] => Ok(Ir::Quote(lower_quoted(single)?)),
            _ => Err(LowerError::arity("quote expects exactly one argument")),
        };
    }
    if sid == my_lisp::sid!(00000111) {
        return lower_cond(args, env);
    }
    if sid == my_lisp::sid!(00001000) && args.len() >= 2 {
        return lower_lambda(args, env);
    }
    if (sid == my_lisp::sid!(00001001) || sid == my_lisp::sid!(00001011)) && args.len() == 2 {
        return lower_def(args, env);
    }
    if sid == my_lisp::sid!(10011101) && args.len() == 2 {
        return lower_let_star(args, env);
    }
    if sid == my_lisp::sid!(00010100) && args.len() == 2 {
        return lower_prim(PrimOp::Quotient, args, env);
    }
    if sid == my_lisp::sid!(00001010) {
        return Err(LowerError::invalid_form(
            "defmacro must be expanded before IR lowering",
        ));
    }
    lower_generic_call(&Expr::Sid(sid), args, env)
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
    if func == "let*" && args.len() == 2 {
        return lower_let_star(args, env);
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
            if semantic_id == my_lisp::sid!(00000010) && args.len() == 1 {
                return lower_prim(PrimOp::Atom, args, env);
            }
            if semantic_id == my_lisp::sid!(00000011) && args.len() == 2 {
                return lower_prim(PrimOp::Eq, args, env);
            }
            if semantic_id == my_lisp::sid!(00100111) {
                // LIST is variadic: (list) -> NIL, (list a b c) -> (a b c)
                return lower_prim(PrimOp::List, args, env);
            }
            if semantic_id == my_lisp::sid!(00100111) {
                // LIST is variadic: (list) -> NIL, (list a b c) -> (a b c)
                return lower_prim(PrimOp::List, args, env);
            }
            if semantic_id == my_lisp::sid!(00110011) && args.len() == 1 {
                return lower_prim(PrimOp::Caar, args, env);
            }
            if semantic_id == my_lisp::sid!(00110100) && args.len() == 1 {
                return lower_prim(PrimOp::Cadr, args, env);
            }
            if semantic_id == my_lisp::sid!(00110101) && args.len() == 1 {
                return lower_prim(PrimOp::Cddr, args, env);
            }
            if semantic_id == my_lisp::sid!(00110110) && args.len() == 1 {
                return lower_prim(PrimOp::Cadddr, args, env);
            }
            if semantic_id == my_lisp::sid!(00110011) && args.len() == 1 {
                return lower_prim(PrimOp::Caar, args, env);
            }
            if semantic_id == my_lisp::sid!(00110100) && args.len() == 1 {
                return lower_prim(PrimOp::Cadr, args, env);
            }
            if semantic_id == my_lisp::sid!(00110101) && args.len() == 1 {
                return lower_prim(PrimOp::Cddr, args, env);
            }
            if semantic_id == my_lisp::sid!(00110110) && args.len() == 1 {
                return lower_prim(PrimOp::Cadddr, args, env);
            }
            if semantic_id == my_lisp::sid!(00000100) && args.len() == 2 {
                return lower_prim(PrimOp::Cons, args, env);
            }
            if semantic_id == my_lisp::sid!(00000101) && args.len() == 1 {
                return lower_prim(PrimOp::Car, args, env);
            }
            if semantic_id == my_lisp::sid!(00000110) && args.len() == 1 {
                return lower_prim(PrimOp::Cdr, args, env);
            }
            if semantic_id == my_lisp::sid!(00001100) && args.len() == 2 {
                return lower_prim(PrimOp::Add, args, env);
            }
            if semantic_id == my_lisp::sid!(00001101) && args.len() == 2 {
                return lower_prim(PrimOp::Sub, args, env);
            }
            if semantic_id == my_lisp::sid!(00100010) && args.len() == 2 {
                return lower_prim(PrimOp::EqualP, args, env);
            }
            if semantic_id == my_lisp::sid!(00011010) && args.len() == 2 {
                return lower_prim(PrimOp::ExactQLt, args, env);
            }
            if semantic_id == my_lisp::sid!(00011101) && args.len() == 2 {
                return lower_prim(PrimOp::ExactQLe, args, env);
            }
            if semantic_id == my_lisp::sid!(00011110) && args.len() == 2 {
                return lower_prim(PrimOp::ExactQGe, args, env);
            }
            if semantic_id == my_lisp::sid!(01011001) {
                if args.len() == 2 {
                    return lower_generic_call(
                        &Expr::Symbol("NUMERIC-BUFFER-MAP".to_string()),
                        args,
                        env,
                    );
                }
                return Err(LowerError::arity(
                    "numeric-buffer-map expects exactly two arguments",
                ));
            }
        }
        // (#238) Typed user-def call identity. A word with a Lisp-owned
        // registered semantic ID that is neither a backend primitive projection
        // nor a canonical builtin (e.g. `reverse`, `not`) dispatches by its
        // Sid8 call key, but only when the current program actually keys the
        // definition under that SID. This keeps call identity identical to def
        // identity and preserves classic name-keyed definitions in other
        // backends. Canonical builtins (`=`, `>`, `mod`, ...) keep their
        // Builtin identity on purpose (admitted-but-partial, #92).
        if let Some(sid) = my_lisp::semantic_registry_export::semantic_id_for_admitted_surface(func)
        {
            if crate::canon::canonical_builtin_name(sid).is_none() && env.has_sid_keyed_def(sid) {
                return lower_generic_call(&Expr::Sid(sid), args, env);
            }
        }
        // Other admitted callables that are primitive operations but not in
        // the Canon callable table (e.g. `quotient`) lower directly to PrimOp.
        if let Some(sid) = my_lisp::semantic_registry_export::semantic_id_for_admitted_surface(func)
        {
            if sid == my_lisp::sid!(00010100) && args.len() == 2 {
                return lower_prim(PrimOp::Quotient, args, env);
            }
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
    enum CondShape {
        Compatibility(Vec<(Ir, Ir)>),
        Canonical(Vec<(Ir, Quoted, Ir)>),
    }

    let mut shape: Option<CondShape> = None;
    for branch in branches {
        let Expr::List(parts) = branch else {
            return Err(LowerError::invalid_form("cond expects list clauses"));
        };

        match (shape.take(), parts.as_slice()) {
            (None, [query, expected, body]) => {
                shape = Some(CondShape::Canonical(vec![(
                    lower_expr_admitted(query, env)?,
                    lower_quoted(expected)?,
                    lower_expr_admitted(body, env)?,
                )]));
            }
            (Some(CondShape::Canonical(mut lowered)), [query, expected, body]) => {
                lowered.push((
                    lower_expr_admitted(query, env)?,
                    lower_quoted(expected)?,
                    lower_expr_admitted(body, env)?,
                ));
                shape = Some(CondShape::Canonical(lowered));
            }
            (None, [test, body]) => {
                shape = Some(CondShape::Compatibility(vec![(
                    lower_expr_admitted(test, env)?,
                    lower_expr_admitted(body, env)?,
                )]));
            }
            (Some(CondShape::Compatibility(mut lowered)), [test, body]) => {
                lowered.push((
                    lower_expr_admitted(test, env)?,
                    lower_expr_admitted(body, env)?,
                ));
                shape = Some(CondShape::Compatibility(lowered));
            }
            (Some(existing), [_, _, _] | [_, _]) => {
                let _ = existing;
                return Err(LowerError::invalid_form(
                    "cond cannot mix canonical three-part clauses with migration-only two-part clauses",
                ));
            }
            (_, _) => {
                return Err(LowerError::invalid_form(
                    "cond expects canonical (query expected-result expression) clauses or migration-only (test expression) clauses",
                ));
            }
        }
    }

    match shape {
        Some(CondShape::Canonical(branches)) => Ok(Ir::CondMatch { branches }),
        Some(CondShape::Compatibility(branches)) => Ok(Ir::Cond { branches }),
        None => Ok(Ir::CondMatch {
            branches: Vec::new(),
        }),
    }
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

/// Sequential `let*`: each binding sees the previous ones.
/// `(let* ((n1 v1) (n2 v2) ...) body)` expands to nested parallel `let`s:
/// `(let ((n1 v1)) (let ((n2 v2)) ... body))`.
fn lower_let_star(args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    let [bindings_expr, body_expr] = args else {
        return Err(LowerError::arity("let* expects exactly two arguments"));
    };
    let Expr::List(bindings) = bindings_expr else {
        return Err(LowerError::invalid_form("let* expects a binding list"));
    };
    if bindings.is_empty() {
        return lower_expr_admitted(body_expr, env);
    }
    let mut iter = bindings.iter();
    let first = iter.next().unwrap();
    let rest: Vec<Expr> = iter.cloned().collect();
    let Expr::List(pair) = first else {
        return Err(LowerError::invalid_form("let* binding must be a list"));
    };
    let [name_expr, value_expr] = pair.as_slice() else {
        return Err(LowerError::invalid_form(
            "let* binding must be (name value)",
        ));
    };
    let Expr::Symbol(name) = name_expr else {
        return Err(LowerError::invalid_form(
            "let* binding name must be a symbol",
        ));
    };
    let inner_body = if rest.is_empty() {
        body_expr.clone()
    } else {
        Expr::List(vec![
            Expr::Symbol("let*".to_string()),
            Expr::List(rest),
            body_expr.clone(),
        ])
    };
    let expanded = Expr::List(vec![
        Expr::Symbol("let".to_string()),
        Expr::List(vec![Expr::List(vec![
            Expr::Symbol(name.to_string()),
            value_expr.clone(),
        ])]),
        inner_body,
    ]);
    lower_expr_admitted(&expanded, env)
}

fn lower_def(args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    let name = match &args[0] {
        Expr::Symbol(name) => name.to_uppercase(),
        // A SID-keyed define uses the typed Sid8's own bit pattern as the
        // def key (key_definition_by_sid, #239) -- no surface registry.
        Expr::Sid(sid) => sid.to_string(),
        _ => {
            return Err(LowerError::invalid_form(
                "def expects a symbol or SID8 name",
            ));
        }
    };
    let value = lower_expr_admitted(&args[1], env)?;
    Ok(Ir::Def {
        name,
        value: Box::new(value),
    })
}

fn lower_quoted(expr: &Expr) -> Result<Quoted, LowerError> {
    match expr {
        Expr::Sid(_) => Err(LowerError::invalid_form(
            "quoted SID8 is not a function identity; quoted/string/literal SID wrappers are forbidden",
        )),
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

#[cfg(test)]
mod sid_head_tests {
    use super::*;

    #[test]
    fn sid_headed_lambda_lowers_like_surface_lambda() {
        let expr = Expr::List(vec![
            Expr::Sid(my_lisp::sid!(00001000)),
            Expr::Symbol("args".into()),
            Expr::Symbol("args".into()),
        ]);
        let ir = lower_expr_admitted(&expr, &Env::default()).expect("SID lambda lowers");
        assert!(matches!(
            ir,
            Ir::Lambda {
                params: Params::AllRest(ref rest),
                ..
            } if rest == "ARGS"
        ));
    }

    #[test]
    fn sid_headed_quote_lowers_to_quote() {
        let expr = Expr::List(vec![Expr::Sid(my_lisp::sid!(00000001)), Expr::List(vec![])]);
        let ir = lower_expr_admitted(&expr, &Env::default()).expect("SID quote lowers");
        assert!(matches!(ir, Ir::Quote(Quoted::Nil)));
    }

    #[test]
    fn sid_headed_define_row_keys_def_by_typed_sid_bits() {
        let expr = Expr::List(vec![
            Expr::Sid(my_lisp::sid!(00001001)),
            Expr::Sid(my_lisp::sid!(00100111)),
            Expr::List(vec![
                Expr::Sid(my_lisp::sid!(00001000)),
                Expr::Symbol("args".into()),
                Expr::Symbol("args".into()),
            ]),
        ]);
        let ir = lower_expr_admitted(&expr, &Env::default()).expect("SID define row lowers");
        match ir {
            Ir::Def { name, value } => {
                assert_eq!(name, "00100111");
                assert!(matches!(
                    *value,
                    Ir::Lambda {
                        params: Params::AllRest(_),
                        ..
                    }
                ));
            }
            other => panic!("expected Ir::Def, got {other:?}"),
        }
    }

    #[test]
    fn top_level_sid_define_registers_env_binding() {
        let expr = Expr::List(vec![
            Expr::Sid(my_lisp::sid!(00001001)),
            Expr::Sid(my_lisp::sid!(00100111)),
            Expr::List(vec![]),
        ]);
        assert_eq!(
            top_level_definition_name(&expr),
            Some("00100111".to_string())
        );
    }

    #[test]
    fn non_special_sid_head_stays_a_direct_call_value() {
        let expr = Expr::List(vec![Expr::Sid(my_lisp::sid!(00000101)), Expr::List(vec![])]);
        let ir = lower_expr_admitted(&expr, &Env::default()).expect("SID call lowers");
        assert!(matches!(
            ir,
            Ir::App { ref func, .. }
                if matches!(func.as_ref(), Ir::Sid(s) if *s == my_lisp::sid!(00000101))
        ));
    }

    #[test]
    fn user_word_call_resolves_to_registry_sid_identity() {
        // A user-defined word with a Lisp-owned registered semantic ID that is
        // neither a backend primitive nor a canonical builtin (e.g. `reverse`)
        // dispatches by its typed Sid8 call key when the program keys the def
        // under that SID: `(reverse x)` -> App(Sid(...)).
        let mut env = Env::default();
        let sid = my_lisp::semantic_registry_export::semantic_id_for_admitted_surface("reverse")
            .expect("reverse has an admitted surface SID");
        env.sid_keyed_defs.insert(sid.to_string());
        let expr = Expr::List(vec![
            Expr::Symbol("reverse".into()),
            Expr::Symbol("x".into()),
        ]);
        let ir = lower_expr_admitted(&expr, &env).expect("user-word call lowers");
        match ir {
            Ir::App { func, args } => {
                assert!(matches!(
                    func.as_ref(),
                    Ir::Sid(sid) if *sid
                        == my_lisp::semantic_registry_export::semantic_id_for_admitted_surface(
                            "reverse"
                        )
                        .expect("reverse has an admitted surface SID")
                ));
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected typed SID App, got {other:?}"),
        }
    }

    #[test]
    fn equality_builtin_stays_builtin_identity_not_sid() {
        // #92 contract: numeric `=` keeps its admitted-but-partial canonical
        // Builtin identity; the typed SID route must never capture it.
        let expr = Expr::List(vec![
            Expr::Symbol("=".into()),
            Expr::Symbol("x".into()),
            Expr::Integer(1),
        ]);
        let ir = lower_expr_admitted(&expr, &Env::default()).expect("builtin call lowers");
        match ir {
            Ir::App { func, .. } => {
                assert!(matches!(func.as_ref(), Ir::Builtin(name) if name == "="));
            }
            other => panic!("expected Builtin App, got {other:?}"),
        }
    }

    #[test]
    fn let_star_expands_to_nested_parallel_let() {
        // `(let* ((x 1) (y (+ x 1))) y)` -> nested Ir::Let.
        let expr = Expr::List(vec![
            Expr::Symbol("let*".into()),
            Expr::List(vec![
                Expr::List(vec![Expr::Symbol("x".into()), Expr::Integer(1)]),
                Expr::List(vec![
                    Expr::Symbol("y".into()),
                    Expr::List(vec![
                        Expr::Symbol("+".into()),
                        Expr::Symbol("x".into()),
                        Expr::Integer(1),
                    ]),
                ]),
            ]),
            Expr::Symbol("y".into()),
        ]);
        let ir = lower_expr_admitted(&expr, &Env::default()).expect("let* lowers");
        match ir {
            Ir::Let {
                bindings: outer_bindings,
                body,
            } => {
                assert_eq!(outer_bindings.len(), 1);
                assert_eq!(outer_bindings[0].0, "X");
                match body.as_ref() {
                    Ir::Let {
                        bindings: inner_bindings,
                        body: inner_body,
                    } => {
                        assert_eq!(inner_bindings.len(), 1);
                        assert_eq!(inner_bindings[0].0, "Y");
                        assert!(matches!(inner_body.as_ref(), Ir::Var(name) if name == "Y"));
                    }
                    other => panic!("expected nested let, got {other:?}"),
                }
            }
            other => panic!("expected Ir::Let, got {other:?}"),
        }
    }

    #[test]
    fn quotient_call_lowers_to_primop() {
        // Both surface `quotient` and SID 00010100 lower to PrimOp::Quotient.
        let surface = Expr::List(vec![
            Expr::Symbol("quotient".into()),
            Expr::Integer(7),
            Expr::Integer(2),
        ]);
        let ir = lower_expr_admitted(&surface, &Env::default()).expect("quotient lowers");
        assert!(matches!(
            ir,
            Ir::Prim {
                op: PrimOp::Quotient,
                args
            } if args.len() == 2
        ));

        let sid_call = Expr::List(vec![
            Expr::Sid(my_lisp::sid!(00010100)),
            Expr::Integer(7),
            Expr::Integer(2),
        ]);
        let ir = lower_expr_admitted(&sid_call, &Env::default()).expect("SID quotient lowers");
        assert!(matches!(
            ir,
            Ir::Prim {
                op: PrimOp::Quotient,
                args
            } if args.len() == 2
        ));
    }
}
