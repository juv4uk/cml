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
use crate::ir::{BufferLiteral, Ir, MachineOp, Params, Quoted};
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
        Expr::Sid(sid) => *sid == sens::sens!(00001001) || *sid == sens::sens!(00001011),
        _ => false,
    };
    if !is_define_head {
        return None;
    }
    match name {
        Expr::Symbol(s) => Some(s.to_uppercase()),
        // A SID-keyed define (key_definition_by_sid after #239) uses the
        // typed Sens8's own bit pattern as the def's internal key -- no
        // surface/registry lookup.
        Expr::Sid(sid) => Some(sid.to_string()),
        _ => None,
    }
}

/// Returns the SID used as a definition key, if this top-level form is a
/// define whose name is a typed Sens8. Name-keyed surface defines return `None`.
fn sid_keyed_definition_name(expr: &Expr) -> Option<sens::Sens8> {
    use crate::canon::{CANON_DEFINE_EXACT, CANON_DEFINE_UPPER, is_canon_form};

    let Expr::List(items) = expr else {
        return None;
    };
    let [head, name, _] = items.as_slice() else {
        return None;
    };
    let is_define_head = match head {
        Expr::Symbol(form) => is_canon_form(form, CANON_DEFINE_UPPER, CANON_DEFINE_EXACT),
        Expr::Sid(sid) => *sid == sens::sens!(00001001) || *sid == sens::sens!(00001011),
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
            sid_keyed_defs.insert(sid);
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

#[derive(Clone, Default)]
struct Env {
    bound: Vec<String>,
    /// Set of definition keys that are typed 8-bit SID patterns (from
    /// byte-SID `define` rows). A name-keyed call is lowered to `App(Sid)`
    /// only when the target def is actually registered under that SID, so
    /// the call identity matches the def identity (#238).
    sid_keyed_defs: std::collections::HashSet<sens::Sens8>,
}

impl Env {
    fn is_bound(&self, name: &str) -> bool {
        self.bound.iter().any(|b| b == name)
    }

    fn has_sid_keyed_def(&self, sid: sens::Sens8) -> bool {
        self.sid_keyed_defs.contains(&sid)
    }
}

pub fn lower_expr(expr: &Expr) -> Result<Ir, LowerError> {
    semantic::analyze_expr(expr).map_err(LowerError::semantic)?;
    lower_expr_admitted(expr, &Env::default())
}

fn lower_expr_admitted(expr: &Expr, env: &Env) -> Result<Ir, LowerError> {
    match expr {
        Expr::Sid(sid) => Ok(Ir::Sid(*sid)),
        Expr::DomainIdentity(identity) => Err(LowerError::invalid_form(format!(
            "exact DomainIdentity D{}:{:0width$b} reached AST but the canonical IR lane is not admitted yet (#406)",
            identity.width(),
            identity.packed_bits(),
            width = identity.width()
        ))),
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
    // #246: the identity is the exact Sens8, never Builtin or surface text.
    if !env.is_bound(&upper) {
        if let Some(semantic_id) = crate::canon::callable_semantic_id(s) {
            return Ok(Ir::Sid(semantic_id));
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
    } else if let Expr::DomainIdentity(identity) = &list[0] {
        lower_domain_head(*identity, &list[1..], env)
    } else {
        lower_generic_call(&list[0], &list[1..], env)
    }
}

fn lower_domain_prim(
    op: PrimOp,
    name: &'static str,
    arity: usize,
    args: &[Expr],
    env: &Env,
) -> Result<Ir, LowerError> {
    if args.len() != arity {
        return Err(LowerError::arity(format!(
            "{name} expects exactly {arity} argument(s)"
        )));
    }
    Ok(Ir::Prim {
        op,
        args: args
            .iter()
            .map(|expr| lower_expr_admitted(expr, env))
            .collect::<Result<_, _>>()?,
    })
}

/// Current exact-domain head lowering. This is deliberately narrower than the
/// upstream residency set: only D3 mechanisms with an already-existing CML IR
/// representation are admitted here. The semantic identity is consumed once
/// at compile time and does not become a second backend registry.
fn lower_domain_head(
    identity: sens::DomainIdentity,
    args: &[Expr],
    env: &Env,
) -> Result<Ir, LowerError> {
    if identity.width() != 3 {
        return Err(LowerError::invalid_form(format!(
            "exact DomainIdentity D{}:{:0width$b} is not admitted by the first canonical lowering slice",
            identity.width(),
            identity.packed_bits(),
            width = identity.width()
        )));
    }

    match identity.packed_bits() {
        0b000 => Err(LowerError::invalid_form(
            "D3 000 structural empty cannot head a call",
        )),
        0b001 => match args {
            [single] => Ok(Ir::Quote(lower_quoted(single)?)),
            _ => Err(LowerError::arity("D3 QUOTE expects exactly one argument")),
        },
        0b010 => lower_domain_prim(PrimOp::Atom, "D3 ATOM", 1, args, env),
        0b011 => lower_domain_prim(PrimOp::Cdr, "D3 CDR", 1, args, env),
        0b100 => lower_domain_prim(PrimOp::Car, "D3 CAR", 1, args, env),
        0b101 => lower_domain_prim(PrimOp::Eq, "D3 EQ", 2, args, env),
        0b110 => Err(LowerError::invalid_form(
            "D3 COND is blocked until CML carries Contract 11.6 two-part exact-PredicateBit control explicitly",
        )),
        0b111 => lower_domain_prim(PrimOp::Cons, "D3 CONS", 2, args, env),
        _ => unreachable!("D3 payload is exactly three bits"),
    }
}

/// Typed SID-head routing for the special forms, mirroring `lower_call`'s
/// Canon-surface dispatch so byte-SID-authored source (pinned `core.lisp`
/// rows such as `(00001000 args args)`) lowers through the same mechanism
/// as the surface spellings. Only the special-form SIDs are intercepted
/// here; every other SID remains a first-class call value and keeps
/// `Ir::App { func: Ir::Sid(sid), .. }` for the direct-SENS code dispatch
/// (#238), never a SID-to-name fallback.
fn lower_sid_head(sid: sens::Sens8, args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    if sid == sens::sens!(00000001) {
        return match args {
            [single] => Ok(Ir::Quote(lower_quoted(single)?)),
            _ => Err(LowerError::arity("quote expects exactly one argument")),
        };
    }
    if sid == sens::sens!(00000111) {
        return lower_cond(args, env);
    }
    if sid == sens::sens!(00001000) && args.len() >= 2 {
        return lower_lambda(args, env);
    }
    if (sid == sens::sens!(00001001) || sid == sens::sens!(00001011)) && args.len() == 2 {
        return lower_def(args, env);
    }
    if sid == sens::sens!(10011101) && args.len() == 2 {
        return lower_let_star(args, env);
    }
    if sid == sens::sens!(00010100) && args.len() == 2 {
        return lower_sid_call(sid, args, env);
    }
    if sid == sens::sens!(00001010) {
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
        // Canon callable identity comes from my-lisp's registry. Every
        // admitted callable lowers as an exact SENS code function call; backends
        // select their private mechanism from the 8-bit identity directly.
        if let Some(semantic_id) = callable_semantic_id(func) {
            // cml#315: every admitted callable lowers to its exact SENS code.
            // `numeric-buffer-map` is no longer re-materialised as a textual
            // `NUMERIC-BUFFER-MAP` symbol for a backend to dispatch on; the
            // eight bits travel unchanged from here to the backend.
            if semantic_id == sens::sens!(01011001) && args.len() != 2 {
                return Err(LowerError::arity(
                    "numeric-buffer-map expects exactly two arguments",
                ));
            }
            return lower_sid_call(semantic_id, args, env);
        }
        // (#238) Typed user-def call identity. A word with a Lisp-owned
        // registered semantic ID that is not a backend primitive projection
        // dispatches by its Sens8 call key, but only when the current program
        // actually keys the definition under that SID. This keeps call
        // identity identical to def identity.
        if let Some(sid) = sens::semantic_registry_export::semantic_id_for_admitted_surface(func) {
            if env.has_sid_keyed_def(sid) {
                return lower_generic_call(&Expr::Sid(sid), args, env);
            }
            // `quotient` is an admitted primitive that is not in the Canon
            // callable table; it still lowers by its exact SENS code.
            if sid == sens::sens!(00010100) && args.len() == 2 {
                return lower_sid_call(sid, args, env);
            }
        }
        // `caddr` has no canonical SID in the upstream registry (only
        // caar/cadr/cddr/cadddr are registered). It lowers as the composite
        // car(cdr(cdr x)) so no backend ever dispatches a name-keyed caddr.
        if func == "caddr" || func == "CADDR" {
            if args.len() != 1 {
                return Err(LowerError::arity("caddr expects exactly one argument"));
            }
            let arg0 = lower_expr_admitted(&args[0], env)?;
            let cdr = |inner: Ir| Ir::App {
                func: Box::new(Ir::Sid(sens::sens!(00000110))),
                args: vec![inner],
            };
            let car = |inner: Ir| Ir::App {
                func: Box::new(Ir::Sid(sens::sens!(00000101))),
                args: vec![inner],
            };
            return Ok(car(cdr(cdr(arg0))));
        }
        // cml#315: target-ABI mechanism imports (`pci-config-*`, `mmio-*`).
        // These are not language functions — they have no upstream registry
        // identity and must not be given a synthesised SENS code. They lower
        // to the closed compiler-owned `MachineOp` mechanism category so no
        // backend ever selects a runtime by comparing a symbol's text.
        if let Some(op) = target_abi_mechanism(func) {
            let expected = target_abi_mechanism_arity(op);
            if args.len() != expected {
                return Err(LowerError::arity(format!(
                    "{} expects exactly {expected} argument(s)",
                    target_abi_mechanism_name(op)
                )));
            }
            return Ok(Ir::MachinePrim {
                op,
                args: args
                    .iter()
                    .map(|e| lower_expr_admitted(e, env))
                    .collect::<Result<_, _>>()?,
            });
        }
    }
    lower_generic_call(&Expr::Symbol(func.to_string()), args, env)
}

/// Target-ABI mechanism import surfaces admitted by the x86 freestanding
/// backend. cml#315: this is the *only* remaining place where a source
/// spelling selects a mechanism, and it happens once, at the reader boundary.
/// After this point the mechanism travels as a `MachineOp` value, never as
/// text.
fn target_abi_mechanism(func: &str) -> Option<MachineOp> {
    match func {
        "pci-config-capability" | "PCI-CONFIG-CAPABILITY" => Some(MachineOp::PciConfigCapability),
        "pci-config-read16" | "PCI-CONFIG-READ16" => Some(MachineOp::PciConfigRead16),
        "mmio-capability" | "MMIO-CAPABILITY" => Some(MachineOp::MmioCapability),
        "mmio-read32" | "MMIO-READ32" => Some(MachineOp::MmioRead32),
        "mmio-write32" | "MMIO-WRITE32" => Some(MachineOp::MmioWrite32),
        _ => None,
    }
}

fn target_abi_mechanism_name(op: MachineOp) -> &'static str {
    match op {
        MachineOp::PciConfigCapability => "pci-config-capability",
        MachineOp::PciConfigRead16 => "pci-config-read16",
        MachineOp::MmioCapability => "mmio-capability",
        MachineOp::MmioRead32 => "mmio-read32",
        MachineOp::MmioWrite32 => "mmio-write32",
        MachineOp::Rdtsc => "rdtsc",
    }
}

fn target_abi_mechanism_arity(op: MachineOp) -> usize {
    match op {
        MachineOp::PciConfigCapability | MachineOp::MmioCapability => 0,
        MachineOp::PciConfigRead16 => 5,
        MachineOp::MmioRead32 => 2,
        MachineOp::MmioWrite32 => 3,
        MachineOp::Rdtsc => 0,
    }
}

fn lower_sid_call(sid: sens::Sens8, args: &[Expr], env: &Env) -> Result<Ir, LowerError> {
    Ok(Ir::App {
        func: Box::new(Ir::Sid(sid)),
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
        // A SID-keyed define uses the typed Sens8's own bit pattern as the
        // def key (key_definition_by_sid, #239) -- no surface registry.
        Expr::Sid(sid) => sid.to_string(),
        _ => {
            return Err(LowerError::invalid_form(
                "def expects a symbol or SENS code name",
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
            "quoted SENS code is not a function identity; quoted/string/literal SID wrappers are forbidden",
        )),
        Expr::DomainIdentity(identity) => Err(LowerError::invalid_form(format!(
            "quoted exact DomainIdentity D{}:{:0width$b} has no canonical CML Quoted representation yet",
            identity.width(),
            identity.packed_bits(),
            width = identity.width()
        ))),
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
mod exact_domain_lowering_tests {
    use super::*;

    fn one(source: &str) -> Expr {
        let mut exprs = crate::parser::parse_canonical_binary(source)
            .expect("upstream canonical source must parse");
        assert_eq!(exprs.len(), 1);
        exprs.remove(0)
    }

    #[test]
    fn real_d3_quote_empty_case_lowers_without_sid8_projection() {
        let expr = one("10 001 00 10 01 01");
        let ir = lower_expr(&expr).expect("exact D3 quote case lowers");
        assert!(matches!(ir, Ir::Quote(Quoted::Nil)));
    }

    #[test]
    fn real_d3_car_empty_case_compiles_identity_away_to_private_car_mechanism() {
        let expr = one("10 100 00 10 001 00 10 10 01 01 01 01");
        let ir = lower_expr(&expr).expect("exact D3 CAR case lowers");
        let Ir::Prim { op: PrimOp::Car, args } = ir else {
            panic!("expected private CAR mechanism IR");
        };
        assert_eq!(args.len(), 1);
        assert!(matches!(args[0], Ir::Quote(Quoted::Nil)));
    }

    #[test]
    fn current_d3_cond_fails_closed_instead_of_using_stale_cond_ir() {
        let expr = one("10 110 01");
        let error = lower_expr(&expr).expect_err("D3 COND must remain blocked in this slice");
        assert!(
            error.to_string().contains("Contract 11.6 two-part exact-PredicateBit"),
            "{error}"
        );
    }

    #[test]
    fn d4_head_is_not_silently_truncated_or_zero_padded_to_d3_or_sid8() {
        let expr = one("10 0001 00 10 01 01");
        let error = lower_expr(&expr).expect_err("D4 is outside first exact-domain lowering slice");
        assert!(error.to_string().contains("D4:0001"), "{error}");
    }
}

#[cfg(test)]
mod sid_head_tests {
    use super::*;

    #[test]
    fn sid_headed_lambda_lowers_like_surface_lambda() {
        let expr = Expr::List(vec![
            Expr::Sid(sens::sens!(00001000)),
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
        let expr = Expr::List(vec![Expr::Sid(sens::sens!(00000001)), Expr::List(vec![])]);
        let ir = lower_expr_admitted(&expr, &Env::default()).expect("SID quote lowers");
        assert!(matches!(ir, Ir::Quote(Quoted::Nil)));
    }

    #[test]
    fn sid_headed_define_row_keys_def_by_typed_sid_bits() {
        let expr = Expr::List(vec![
            Expr::Sid(sens::sens!(00001001)),
            Expr::Sid(sens::sens!(00100111)),
            Expr::List(vec![
                Expr::Sid(sens::sens!(00001000)),
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
            Expr::Sid(sens::sens!(00001001)),
            Expr::Sid(sens::sens!(00100111)),
            Expr::List(vec![]),
        ]);
        assert_eq!(
            top_level_definition_name(&expr),
            Some("00100111".to_string())
        );
    }

    #[test]
    fn non_special_sid_head_stays_a_direct_call_value() {
        let expr = Expr::List(vec![Expr::Sid(sens::sens!(00000101)), Expr::List(vec![])]);
        let ir = lower_expr_admitted(&expr, &Env::default()).expect("SID call lowers");
        assert!(matches!(
            ir,
            Ir::App { ref func, .. }
                if matches!(func.as_ref(), Ir::Sid(s) if *s == sens::sens!(00000101))
        ));
    }

    #[test]
    fn user_word_call_resolves_to_registry_sid_identity() {
        // A user-defined word with a Lisp-owned registered semantic ID that is
        // neither a backend primitive nor a canonical builtin (e.g. `reverse`)
        // dispatches by its typed Sens8 call key when the program keys the def
        // under that SID: `(reverse x)` -> App(Sid(...)).
        let mut env = Env::default();
        let sid = sens::semantic_registry_export::semantic_id_for_admitted_surface("reverse")
            .expect("reverse has an admitted surface SID");
        env.sid_keyed_defs.insert(sid);
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
                        == sens::semantic_registry_export::semantic_id_for_admitted_surface(
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
    fn equality_builtin_lowers_to_sens8() {
        // #246: every admitted callable is an exact SENS code function call.
        // Numeric `=` is SID 00011100; it no longer keeps a Builtin identity.
        let expr = Expr::List(vec![
            Expr::Symbol("=".into()),
            Expr::Symbol("x".into()),
            Expr::Integer(1),
        ]);
        let ir = lower_expr_admitted(&expr, &Env::default()).expect("builtin call lowers");
        match ir {
            Ir::App { func, args } => {
                assert!(matches!(func.as_ref(), Ir::Sid(sid) if *sid == sens::sens!(00011100)));
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected SENS code App, got {other:?}"),
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
    fn quotient_call_lowers_to_sens8() {
        // Both surface `quotient` and SID 00010100 lower to an exact SENS code call.
        let surface = Expr::List(vec![
            Expr::Symbol("quotient".into()),
            Expr::Integer(7),
            Expr::Integer(2),
        ]);
        let ir = lower_expr_admitted(&surface, &Env::default()).expect("quotient lowers");
        match ir {
            Ir::App { func, args } => {
                assert!(matches!(func.as_ref(), Ir::Sid(sid) if *sid == sens::sens!(00010100)));
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected SENS code App, got {other:?}"),
        }

        let sid_call = Expr::List(vec![
            Expr::Sid(sens::sens!(00010100)),
            Expr::Integer(7),
            Expr::Integer(2),
        ]);
        let ir = lower_expr_admitted(&sid_call, &Env::default()).expect("SID quotient lowers");
        match ir {
            Ir::App { func, args } => {
                assert!(matches!(func.as_ref(), Ir::Sid(sid) if *sid == sens::sens!(00010100)));
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected SENS code App, got {other:?}"),
        }
    }

    #[test]
    fn caddr_lowers_as_composite_sens8_accessor_nested_car_cdr_cdr() {
        // caddr has no canonical SID upstream; lowering must compose
        // car(cdr(cdr x)) out of the registered SIDs, never a name-keyed call.
        let surface = Expr::List(vec![Expr::Symbol("caddr".into()), Expr::Symbol("x".into())]);
        let ir = lower_expr_admitted(&surface, &Env::default()).expect("caddr lowers as composite");
        match ir {
            Ir::App { func, args } => {
                assert!(
                    matches!(func.as_ref(), Ir::Sid(sid) if *sid == sens::sens!(00000101)),
                    "outer must be car SID 00000101, got {func:?}"
                );
                assert_eq!(args.len(), 1);
                match &args[0] {
                    Ir::App { func, args } => {
                        assert!(
                            matches!(func.as_ref(), Ir::Sid(sid) if *sid == sens::sens!(00000110)),
                            "middle must be cdr SID 00000110, got {func:?}"
                        );
                        assert_eq!(args.len(), 1);
                        match &args[0] {
                            Ir::App { func, args } => {
                                assert!(
                                    matches!(func.as_ref(), Ir::Sid(sid) if *sid == sens::sens!(00000110)),
                                    "inner must be cdr SID 00000110, got {func:?}"
                                );
                                assert!(matches!(args.as_slice(), [Ir::Var(name)] if name == "X"));
                            }
                            other => panic!("expected inner cdr App, got {other:?}"),
                        }
                    }
                    other => panic!("expected middle cdr App, got {other:?}"),
                }
            }
            other => panic!("expected SENS code car App, got {other:?}"),
        }
    }
}
