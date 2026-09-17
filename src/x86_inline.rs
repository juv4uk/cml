//! Proof-driven callable inlining for x86 Lowered IR (#58).
//!
//! # Architecture & Scope (ADR-004 & Issue #58)
//!
//! Under ADR-004 and the Native Performance Roadmap (#51), this module implements
//! semantics-preserving callable inlining across lowered boundaries.
//!
//! ## Authority & Invariants
//! - `my-lisp` owns language semantics, callable identity, and evaluation contracts.
//! - Backend-neutral `Ir` (`src/ir.rs`) remains completely untouched (no compiler-monopoly tables).
//! - All inlining decisions, recursion guards, and alpha-renaming live in this module.
//!
//! ## Inlining Eligibility Policy
//! A callable is eligible for inlining if and only if all of the following conditions hold:
//! 1. Exact callable identity is unambiguous (lexical lambda or known `Def`/`Let` binding).
//! 2. Arity is known and matches the call site arguments exactly.
//! 3. Recursion is strictly bounded: self-recursive or mutually-recursive calls are not blindly expanded.
//! 4. Evaluation order is strictly preserved (arguments evaluated once in order).
//! 5. Size/cost is within the configured threshold (`max_inline_cost`).
//! 6. Deterministic `--inline=off` mode is preserved for differential testing.

use crate::ir::{Ir, Params};
use std::collections::HashMap;

/// Configuration for the callable inliner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineConfig {
    pub enabled: bool,
    pub max_inline_cost: usize,
    pub max_depth: usize,
}

impl InlineConfig {
    /// Inlining completely disabled.
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            max_inline_cost: 0,
            max_depth: 0,
        }
    }

    /// Standard inlining enabled with conservative defaults.
    pub const fn default_enabled() -> Self {
        Self {
            enabled: true,
            max_inline_cost: 30,
            max_depth: 3,
        }
    }
}

/// Decision provenance recorded for every call site examined by the inliner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineDecision {
    pub callee: String,
    pub inlined: bool,
    pub cost: usize,
    pub reason: String,
}

/// Computes the structural node count/cost of an IR expression.
pub fn ir_cost(ir: &Ir) -> usize {
    match ir {
        Ir::Int(_)
        | Ir::Float(_)
        | Ir::Rational(..)
        | Ir::String(_)
        | Ir::Buffer(_)
        | Ir::Nil
        | Ir::True
        | Ir::Var(_)
        | Ir::Builtin(_)
        | Ir::Quote(_) => 1,
        Ir::Lambda { body, .. } => 1 + ir_cost(body),
        Ir::App { func, args } => 1 + ir_cost(func) + args.iter().map(ir_cost).sum::<usize>(),
        Ir::Cond { branches } => {
            1 + branches
                .iter()
                .map(|(t, b)| ir_cost(t) + ir_cost(b))
                .sum::<usize>()
        }
        Ir::CondMatch { branches } => {
            // `expected` is inert quoted data, not executable IR. Count the
            // branch structure plus executable query/body nodes only.
            1 + branches
                .iter()
                .map(|(query, _expected, body)| ir_cost(query) + ir_cost(body))
                .sum::<usize>()
        }
        Ir::Let { bindings, body } => {
            1 + bindings.iter().map(|(_, v)| ir_cost(v)).sum::<usize>() + ir_cost(body)
        }
        Ir::Def { value, .. } => 1 + ir_cost(value),
        Ir::Prim { args, .. } | Ir::MachinePrim { args, .. } | Ir::TailSelfCall { args } => {
            1 + args.iter().map(ir_cost).sum::<usize>()
        }
    }
}

/// Renames free occurrences of variable names in an expression.
fn substitute_vars(ir: &Ir, mapping: &HashMap<String, String>) -> Ir {
    match ir {
        Ir::Var(name) => {
            if let Some(fresh) = mapping.get(name) {
                Ir::Var(fresh.clone())
            } else {
                Ir::Var(name.clone())
            }
        }
        Ir::Int(n) => Ir::Int(*n),
        Ir::Float(f) => Ir::Float(*f),
        Ir::Rational(n, d) => Ir::Rational(*n, *d),
        Ir::String(s) => Ir::String(s.clone()),
        Ir::Buffer(b) => Ir::Buffer(b.clone()),
        Ir::Nil => Ir::Nil,
        Ir::True => Ir::True,
        Ir::Builtin(b) => Ir::Builtin(b.clone()),
        Ir::Quote(q) => Ir::Quote(q.clone()),
        Ir::Prim { op, args } => Ir::Prim {
            op: *op,
            args: args.iter().map(|a| substitute_vars(a, mapping)).collect(),
        },
        Ir::MachinePrim { op, args } => Ir::MachinePrim {
            op: *op,
            args: args.iter().map(|a| substitute_vars(a, mapping)).collect(),
        },
        Ir::TailSelfCall { args } => Ir::TailSelfCall {
            args: args.iter().map(|a| substitute_vars(a, mapping)).collect(),
        },
        Ir::Cond { branches } => Ir::Cond {
            branches: branches
                .iter()
                .map(|(t, b)| (substitute_vars(t, mapping), substitute_vars(b, mapping)))
                .collect(),
        },
        Ir::CondMatch { branches } => Ir::CondMatch {
            branches: branches
                .iter()
                .map(|(query, expected, body)| {
                    (
                        substitute_vars(query, mapping),
                        expected.clone(),
                        substitute_vars(body, mapping),
                    )
                })
                .collect(),
        },
        Ir::Let { bindings, body } => {
            // Bindings can shadow variables
            let mut inner_mapping = mapping.clone();
            let mut new_bindings = Vec::new();
            for (name, val) in bindings {
                inner_mapping.remove(name);
                new_bindings.push((name.clone(), substitute_vars(val, mapping)));
            }
            Ir::Let {
                bindings: new_bindings,
                body: Box::new(substitute_vars(body, &inner_mapping)),
            }
        }
        Ir::Def { name, value } => {
            let mut inner_mapping = mapping.clone();
            inner_mapping.remove(name);
            Ir::Def {
                name: name.clone(),
                value: Box::new(substitute_vars(value, &inner_mapping)),
            }
        }
        Ir::Lambda { params, body } => {
            let mut inner_mapping = mapping.clone();
            match params {
                Params::Fixed(names) => {
                    for n in names {
                        inner_mapping.remove(n);
                    }
                }
                Params::Variadic { fixed, rest } => {
                    for n in fixed {
                        inner_mapping.remove(n);
                    }
                    inner_mapping.remove(rest);
                }
                Params::AllRest(rest) => {
                    inner_mapping.remove(rest);
                }
            }
            Ir::Lambda {
                params: params.clone(),
                body: Box::new(substitute_vars(body, &inner_mapping)),
            }
        }
        Ir::App { func, args } => Ir::App {
            func: Box::new(substitute_vars(func, mapping)),
            args: args.iter().map(|a| substitute_vars(a, mapping)).collect(),
        },
    }
}

/// Context tracking callable definitions, recursion stack, and inlining decisions.
struct InlineContext<'a> {
    config: &'a InlineConfig,
    callables: HashMap<String, (Params, Ir)>,
    call_stack: Vec<String>,
    next_id: usize,
    decisions: Vec<InlineDecision>,
}

impl<'a> InlineContext<'a> {
    fn new(config: &'a InlineConfig) -> Self {
        Self {
            config,
            callables: HashMap::new(),
            call_stack: Vec::new(),
            next_id: 1,
            decisions: Vec::new(),
        }
    }

    fn fresh_var(&mut self, orig: &str) -> String {
        let name = format!("{orig}$inlined${}", self.next_id);
        self.next_id += 1;
        name
    }
}

/// Performs recursive inlining over an `Ir` expression.
fn inline_expr(ir: &Ir, ctx: &mut InlineContext, depth: usize) -> Ir {
    if !ctx.config.enabled || depth >= ctx.config.max_depth {
        return ir.clone();
    }

    match ir {
        Ir::Def { name, value } => {
            let opt_val = inline_expr(value, ctx, depth);
            if let Ir::Lambda { params, body } = &opt_val {
                ctx.callables
                    .insert(name.clone(), (params.clone(), (**body).clone()));
                ctx.callables.insert(
                    name.to_ascii_uppercase(),
                    (params.clone(), (**body).clone()),
                );
                ctx.callables.insert(
                    name.to_ascii_lowercase(),
                    (params.clone(), (**body).clone()),
                );
            }
            Ir::Def {
                name: name.clone(),
                value: Box::new(opt_val),
            }
        }
        Ir::Let { bindings, body } => {
            let mut new_bindings = Vec::new();
            for (name, val) in bindings {
                let opt_val = inline_expr(val, ctx, depth);
                if let Ir::Lambda { params, body } = &opt_val {
                    ctx.callables
                        .insert(name.clone(), (params.clone(), (**body).clone()));
                    ctx.callables.insert(
                        name.to_ascii_uppercase(),
                        (params.clone(), (**body).clone()),
                    );
                    ctx.callables.insert(
                        name.to_ascii_lowercase(),
                        (params.clone(), (**body).clone()),
                    );
                }
                new_bindings.push((name.clone(), opt_val));
            }
            let new_body = inline_expr(body, ctx, depth);
            Ir::Let {
                bindings: new_bindings,
                body: Box::new(new_body),
            }
        }
        Ir::App { func, args } => {
            // First recursively inline arguments to preserve evaluation order
            let inlined_args: Vec<Ir> = args.iter().map(|a| inline_expr(a, ctx, depth)).collect();

            // Check if callee is a known named callable
            if let Ir::Var(callee_name) = &**func {
                let found_callable = ctx
                    .callables
                    .get(callee_name)
                    .or_else(|| ctx.callables.get(&callee_name.to_ascii_uppercase()))
                    .or_else(|| ctx.callables.get(&callee_name.to_ascii_lowercase()))
                    .cloned();

                if let Some((params, body)) = found_callable {
                    let cost = ir_cost(&body);

                    // Check recursion (case-insensitive)
                    if ctx
                        .call_stack
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(callee_name))
                    {
                        ctx.decisions.push(InlineDecision {
                            callee: callee_name.clone(),
                            inlined: false,
                            cost,
                            reason: "recursive call prevented by bounded recursion policy"
                                .to_string(),
                        });
                        return Ir::App {
                            func: func.clone(),
                            args: inlined_args,
                        };
                    }

                    // Check cost
                    if cost > ctx.config.max_inline_cost {
                        ctx.decisions.push(InlineDecision {
                            callee: callee_name.clone(),
                            inlined: false,
                            cost,
                            reason: format!(
                                "cost {cost} exceeds compiler inline threshold {}",
                                ctx.config.max_inline_cost
                            ),
                        });
                        return Ir::App {
                            func: func.clone(),
                            args: inlined_args,
                        };
                    }

                    // Check arity
                    match params {
                        Params::Fixed(param_names) => {
                            if param_names.len() != inlined_args.len() {
                                ctx.decisions.push(InlineDecision {
                                    callee: callee_name.clone(),
                                    inlined: false,
                                    cost,
                                    reason: format!(
                                        "arity mismatch: expected {}, got {}",
                                        param_names.len(),
                                        inlined_args.len()
                                    ),
                                });
                                return Ir::App {
                                    func: func.clone(),
                                    args: inlined_args,
                                };
                            }

                            // Perform capture-free alpha-renaming
                            let mut rename_map = HashMap::new();
                            let mut let_bindings = Vec::new();
                            for (p, arg_expr) in param_names.iter().zip(inlined_args) {
                                let fresh = ctx.fresh_var(p);
                                rename_map.insert(p.clone(), fresh.clone());
                                let_bindings.push((fresh, arg_expr));
                            }

                            let renamed_body = substitute_vars(&body, &rename_map);

                            ctx.decisions.push(InlineDecision {
                                callee: callee_name.clone(),
                                inlined: true,
                                cost,
                                reason: "pure small callable inlined successfully".to_string(),
                            });

                            // Push to call stack and recursively inline the inlined body
                            ctx.call_stack.push(callee_name.clone());
                            let expanded = Ir::Let {
                                bindings: let_bindings,
                                body: Box::new(renamed_body),
                            };
                            let result = inline_expr(&expanded, ctx, depth + 1);
                            ctx.call_stack.pop();

                            return result;
                        }
                        _ => {
                            ctx.decisions.push(InlineDecision {
                                callee: callee_name.clone(),
                                inlined: false,
                                cost,
                                reason: "variadic parameters cannot be inlined".to_string(),
                            });
                            return Ir::App {
                                func: func.clone(),
                                args: inlined_args,
                            };
                        }
                    }
                } else {
                    ctx.decisions.push(InlineDecision {
                        callee: callee_name.clone(),
                        inlined: false,
                        cost: 0,
                        reason: "unknown or dynamic callable identity".to_string(),
                    });
                    return Ir::App {
                        func: func.clone(),
                        args: inlined_args,
                    };
                }
            }

            // Direct lambda application: ((lambda (...) body) ...)
            if let Ir::Lambda { params, body } = &**func {
                let cost = ir_cost(body);
                if cost <= ctx.config.max_inline_cost {
                    if let Params::Fixed(param_names) = params {
                        if param_names.len() == inlined_args.len() {
                            let mut rename_map = HashMap::new();
                            let mut let_bindings = Vec::new();
                            for (p, arg_expr) in param_names.iter().zip(inlined_args) {
                                let fresh = ctx.fresh_var(p);
                                rename_map.insert(p.clone(), fresh.clone());
                                let_bindings.push((fresh, arg_expr));
                            }
                            let renamed_body = substitute_vars(body, &rename_map);
                            ctx.decisions.push(InlineDecision {
                                callee: "<anonymous lambda>".to_string(),
                                inlined: true,
                                cost,
                                reason: "direct lambda application inlined".to_string(),
                            });
                            let expanded = Ir::Let {
                                bindings: let_bindings,
                                body: Box::new(renamed_body),
                            };
                            return inline_expr(&expanded, ctx, depth + 1);
                        }
                    }
                }
            }

            Ir::App {
                func: Box::new(inline_expr(func, ctx, depth)),
                args: inlined_args,
            }
        }
        Ir::Cond { branches } => Ir::Cond {
            branches: branches
                .iter()
                .map(|(t, b)| (inline_expr(t, ctx, depth), inline_expr(b, ctx, depth)))
                .collect(),
        },
        Ir::CondMatch { branches } => Ir::CondMatch {
            branches: branches
                .iter()
                .map(|(query, expected, body)| {
                    (
                        inline_expr(query, ctx, depth),
                        expected.clone(),
                        inline_expr(body, ctx, depth),
                    )
                })
                .collect(),
        },
        Ir::Prim { op, args } => Ir::Prim {
            op: *op,
            args: args.iter().map(|a| inline_expr(a, ctx, depth)).collect(),
        },
        other => other.clone(),
    }
}

/// Optimizes an IR program by inlining eligible callables according to the provided configuration.
pub fn inline_program(ir: &Ir, config: &InlineConfig) -> (Ir, Vec<InlineDecision>) {
    let mut ctx = InlineContext::new(config);
    let optimized = inline_expr(ir, &mut ctx, 0);
    (optimized, ctx.decisions)
}
