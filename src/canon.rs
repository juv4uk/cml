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
    // Structural forms are resolved by the parser/lowering rules above the
    // ordinary callable layer; they must never be reclassified as function
    // values merely because they also have registry surfaces.
    if is_canon_form(name, CANON_QUOTE_UPPER, CANON_QUOTE_EXACT)
        || is_canon_form(name, CANON_COND_UPPER, CANON_COND_EXACT)
        || is_canon_form(name, CANON_LAMBDA_UPPER, CANON_LAMBDA_EXACT)
        || is_canon_form(name, CANON_DEFINE_UPPER, CANON_DEFINE_EXACT)
        || is_canon_form(name, CANON_DEFMACRO_UPPER, CANON_DEFMACRO_EXACT)
    {
        return None;
    }

    // my-lisp owns admitted surface -> opaque SID identity. CML does not keep
    // a second callable allowlist. Exact Unicode names are tried first; the
    // ASCII-lowercase fallback preserves ordinary Lisp case-insensitivity for
    // English surfaces without changing Ukrainian/Sanskrit identity.
    sens::semantic_registry_export::semantic_id_for_admitted_surface(name).or_else(|| {
        let folded = name.to_ascii_lowercase();
        (folded != name)
            .then(|| sens::semantic_registry_export::semantic_id_for_admitted_surface(&folded))?
    }).or_else(|| {
        // Traditional Lisp names that map to registry entries with `?` suffix.
        // The registry uses `atom?`, `eq?`, `equal?` but Lisp tradition is
        // `atom`, `eq`, `equal`. This is a CML-local surface spelling concern;
        // the upstream registry authority remains unchanged.
        const TRADITIONAL_TO_REGISTRY: &[(&str, &str)] = &[
            ("atom", "atom?"),
            ("eq", "eq?"),
            ("equal", "equal?"),
        ];
        TRADITIONAL_TO_REGISTRY.iter()
            .find_map(|(trad, reg)| (name.eq_ignore_ascii_case(trad)).then_some(*reg))
            .and_then(|reg| sens::semantic_registry_export::semantic_id_for_admitted_surface(reg))
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
            Some(sens::sid!(00000001))
        } else if is_canon_form(name, CANON_COND_UPPER, CANON_COND_EXACT) {
            Some(sens::sid!(00000111))
        } else if is_canon_form(name, CANON_LAMBDA_UPPER, CANON_LAMBDA_EXACT) {
            Some(sens::sid!(00001000))
        } else if is_canon_form(name, CANON_DEFINE_UPPER, CANON_DEFINE_EXACT) {
            Some(sens::sid!(00001001))
        } else if is_canon_form(name, CANON_DEFMACRO_UPPER, CANON_DEFMACRO_EXACT) {
            Some(sens::sid!(00001010))
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
    use std::collections::HashSet;

    let mut ids = Vec::new();
    let mut seen = HashSet::new();

    fn record(sid: Sid8, ids: &mut Vec<Sid8>, seen: &mut HashSet<Sid8>) {
        if seen.insert(sid) {
            ids.push(sid);
        }
    }

    fn walk(ir: &Ir, ids: &mut Vec<Sid8>, seen: &mut HashSet<Sid8>) {
        match ir {
            Ir::Quote(_) => {
                record(sens::sid!(00000001), ids, seen);
            }
            Ir::Cond { branches } => {
                record(sens::sid!(00000111), ids, seen);
                for (test, body) in branches {
                    walk(test, ids, seen);
                    walk(body, ids, seen);
                }
            }
            Ir::Lambda { body, .. } => {
                record(sens::sid!(00001000), ids, seen);
                walk(body, ids, seen);
            }
            Ir::Def { value, .. } => {
                record(sens::sid!(00001001), ids, seen);
                walk(value, ids, seen);
            }
            Ir::Let { bindings, body } => {
                for (_, val) in bindings {
                    walk(val, ids, seen);
                }
                walk(body, ids, seen);
            }
            Ir::Prim { args, .. } => {
                // #252 / #246: PrimOp is a migration-era mechanism shape, not
                // semantic function identity. Never reconstruct a Sid8 from a
                // host enum here. Current lowering records callable identity as
                // Ir::Sid and Ir::App(func = Ir::Sid(...)); legacy Prim nodes
                // may still be traversed so nested exact identities are visible.
                for arg in args {
                    walk(arg, ids, seen);
                }
            }
            Ir::MachinePrim { args, .. } => {
                // Machine primitives are compiler-owned target mechanisms, NOT language semantic IDs.
                for arg in args {
                    walk(arg, ids, seen);
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
                record(*sid, ids, seen);
            }
            Ir::App { func, args } => {
                walk(func, ids, seen);
                for arg in args {
                    walk(arg, ids, seen);
                }
            }
            _ => {}
        }
    }

    for expr in program {
        walk(expr, &mut ids, &mut seen);
    }

    ids.into_iter().filter_map(find_operation_by_id).collect()
}
