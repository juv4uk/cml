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
pub fn callable_semantic_id(name: &str) -> Option<Sid8> {
    let folded = name.to_uppercase();
    CANON_CALLABLE_UPPER
        .iter()
        .find_map(|(id, surface)| (*surface == folded).then_some(*id))
        .or_else(|| {
            CANON_CALLABLE_EXACT
                .iter()
                .find_map(|(id, surface)| (*surface == name).then_some(*id))
        })
}

/// Map an admitted semantic ID to its canonical uppercase target builtin name.
pub fn canonical_builtin_name(semantic_id: Sid8) -> Option<&'static str> {
    CANON_BUILTIN_NAMES
        .iter()
        .find_map(|(id, name)| (*id == semantic_id).then_some(*name))
}

/// Find the full machine-readable Canon operation entry by surface name.
pub fn find_operation_by_surface(name: &str) -> Option<&'static CanonOperation> {
    let id = callable_semantic_id(name).or_else(|| {
        if is_canon_form(name, CANON_QUOTE_UPPER, CANON_QUOTE_EXACT) {
            Some(my_lisp::sid!(00000001))
        } else if is_canon_form(name, CANON_COND_UPPER, CANON_COND_EXACT) {
            Some(my_lisp::sid!(00000111))
        } else if is_canon_form(name, CANON_LAMBDA_UPPER, CANON_LAMBDA_EXACT) {
            Some(my_lisp::sid!(00001000))
        } else if is_canon_form(name, CANON_DEFINE_UPPER, CANON_DEFINE_EXACT) {
            Some(my_lisp::sid!(00001001))
        } else if is_canon_form(name, CANON_DEFMACRO_UPPER, CANON_DEFMACRO_EXACT) {
            Some(my_lisp::sid!(00001010))
        } else {
            None
        }
    })?;
    find_operation_by_id(id)
}

/// Find the full machine-readable Canon operation entry by semantic ID.
pub fn find_operation_by_id(semantic_id: Sid8) -> Option<&'static CanonOperation> {
    CANON_OPERATIONS_TABLE
        .iter()
        .find(|op| op.semantic_id == semantic_id)
}

/// Collect every unique canonical operation present in an IR expression stream.
pub fn collect_program_operations(program: &[crate::ir::Ir]) -> Vec<&'static CanonOperation> {
    use crate::ir::Ir;
    use std::collections::BTreeSet;

    let mut ids = BTreeSet::new();

    fn walk(ir: &Ir, ids: &mut BTreeSet<Sid8>) {
        match ir {
            Ir::Quote(_) => {
                ids.insert(my_lisp::sid!(00000001));
            }
            Ir::Cond { branches } => {
                ids.insert(my_lisp::sid!(00000111));
                for (test, body) in branches {
                    walk(test, ids);
                    walk(body, ids);
                }
            }
            Ir::Lambda { body, .. } => {
                ids.insert(my_lisp::sid!(00001000));
                walk(body, ids);
            }
            Ir::Def { value, .. } => {
                ids.insert(my_lisp::sid!(00001001));
                walk(value, ids);
            }
            Ir::Let { bindings, body } => {
                for (_, val) in bindings {
                    walk(val, ids);
                }
                walk(body, ids);
            }
            Ir::Prim { args, .. } => {
                // #252 / #246: PrimOp is a migration-era mechanism shape, not
                // semantic function identity. Never reconstruct a Sid8 from a
                // host enum here. Current lowering records callable identity as
                // Ir::Sid and Ir::App(func = Ir::Sid(...)); legacy Prim nodes
                // may still be traversed so nested exact identities are visible.
                for arg in args {
                    walk(arg, ids);
                }
            }
            Ir::MachinePrim { args, .. } => {
                // Machine primitives are compiler-owned target mechanisms, NOT language semantic IDs.
                for arg in args {
                    walk(arg, ids);
                }
            }
            Ir::Builtin(_) => {
                // #252 / #246: a human/backend name cannot be promoted back
                // into semantic function identity. Exact callable identity is
                // collected only from typed Ir::Sid nodes.
            }
            // #246: first-class callables carry the exact Sid8 identity, not a
            // compiler Builtin name. Record the operation so a program that
            // references a callable as a value is credited with that operation.
            Ir::Sid(sid) => {
                ids.insert(*sid);
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
