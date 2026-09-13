//! Generated Canon surface tables and machine-readable operations, read
//! from the real, vendored my-lisp `lib/surface/semantic-registry.wsm`
//! at build time (see `build.rs`).
//!
//! `CANON_UPPER_SURFACES` / `CANON_EXACT_SURFACES` feed `semantic.rs`'s
//! shadowing guard (cml#9). `CANON_<FORM>_UPPER` / `CANON_<FORM>_EXACT` feed
//! `lower.rs`'s special-form dispatch: each pair lists every Canon-registered
//! spelling of one dispatch form across every language the registry admits.
//! `CANON_OPERATIONS_TABLE` provides the complete machine-readable table
//! linking canonical identity, semantic ID, formal action, accepted surfaces,
//! CML IR projection, backend projections, status, authority owner, and witness.

include!(concat!(env!("OUT_DIR"), "/canon_spellings.rs"));

/// True if `name` is the given Canon dispatch form's surface in any
/// registered language: uppercase-folded for `en`/`sym` surfaces (matching
/// ordinary symbol case-insensitivity), exact for `uk`/`sa` surfaces.
pub fn is_canon_form(name: &str, upper: &[&str], exact: &[&str]) -> bool {
    upper.contains(&name.to_uppercase().as_str()) || exact.contains(&name)
}

/// Resolve an admitted Canon callable spelling to the opaque semantic ID
/// generated from my-lisp's authoritative registry. The compiler may choose
/// a mechanism for a known ID, but it does not get to invent identity from a
/// human-facing spelling.
pub fn callable_semantic_id(name: &str) -> Option<&'static str> {
    let folded = name.to_uppercase();
    CANON_CALLABLE_UPPER
        .iter()
        .find_map(|(surface, id)| (*surface == folded).then_some(*id))
        .or_else(|| {
            CANON_CALLABLE_EXACT
                .iter()
                .find_map(|(surface, id)| (*surface == name).then_some(*id))
        })
}

/// Map an admitted semantic ID to its canonical uppercase target builtin name.
pub fn canonical_builtin_name(semantic_id: &str) -> Option<&'static str> {
    CANON_BUILTIN_NAMES
        .iter()
        .find_map(|(id, name)| (*id == semantic_id).then_some(*name))
}

/// Find the full machine-readable Canon operation entry by surface name.
pub fn find_operation_by_surface(name: &str) -> Option<&'static CanonOperation> {
    let id = callable_semantic_id(name).or_else(|| {
        if is_canon_form(name, CANON_QUOTE_UPPER, CANON_QUOTE_EXACT) {
            Some("0001")
        } else if is_canon_form(name, CANON_COND_UPPER, CANON_COND_EXACT) {
            Some("0007")
        } else if is_canon_form(name, CANON_LAMBDA_UPPER, CANON_LAMBDA_EXACT) {
            Some("0010")
        } else if is_canon_form(name, CANON_DEFINE_UPPER, CANON_DEFINE_EXACT) {
            Some("0011")
        } else if is_canon_form(name, CANON_DEFMACRO_UPPER, CANON_DEFMACRO_EXACT) {
            Some("0012")
        } else {
            None
        }
    })?;
    find_operation_by_id(id)
}

/// Find the full machine-readable Canon operation entry by semantic ID.
pub fn find_operation_by_id(semantic_id: &str) -> Option<&'static CanonOperation> {
    CANON_OPERATIONS_TABLE
        .iter()
        .find(|op| op.semantic_id == semantic_id)
}

/// Collect every unique canonical operation present in an IR expression stream.
pub fn collect_program_operations(program: &[crate::ir::Ir]) -> Vec<&'static CanonOperation> {
    use crate::ir::{Ir, PrimOp};
    use std::collections::BTreeSet;

    let mut ids = BTreeSet::new();

    fn walk(ir: &Ir, ids: &mut BTreeSet<&'static str>) {
        match ir {
            Ir::Quote(_) => {
                ids.insert("0001");
            }
            Ir::Cond { branches } => {
                ids.insert("0007");
                for (test, body) in branches {
                    walk(test, ids);
                    walk(body, ids);
                }
            }
            Ir::Lambda { body, .. } => {
                ids.insert("0010");
                walk(body, ids);
            }
            Ir::Def { value, .. } => {
                ids.insert("0011");
                walk(value, ids);
            }
            Ir::Let { bindings, body } => {
                for (_, val) in bindings {
                    walk(val, ids);
                }
                walk(body, ids);
            }
            Ir::Prim { op, args } => {
                let id = match op {
                    PrimOp::Atom => "0002",
                    PrimOp::Eq => "0003",
                    PrimOp::Cons => "0004",
                    PrimOp::Car => "0005",
                    PrimOp::Cdr => "0006",
                    PrimOp::Add => "0104",
                    PrimOp::Sub => "1001",
                    PrimOp::EqualP => "1022",
                };
                ids.insert(id);
                for arg in args {
                    walk(arg, ids);
                }
            }
            Ir::Builtin(name) => {
                if let Some(id) = callable_semantic_id(name) {
                    ids.insert(id);
                }
            }
            Ir::App { func, args } => {
                walk(func, ids);
                for arg in args {
                    walk(arg, ids);
                }
            }
            _ => {}
        }
    }

    for expr in program {
        walk(expr, &mut ids);
    }

    ids.into_iter().filter_map(find_operation_by_id).collect()
}
