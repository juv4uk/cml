//! Generated Canon surface tables, read from the real, vendored my-lisp
//! `lib/surface/semantic-registry.wsm` at build time (see `build.rs`).
//!
//! `CANON_UPPER_SURFACES` / `CANON_EXACT_SURFACES` feed `semantic.rs`'s
//! shadowing guard (cml#9). `CANON_<FORM>_UPPER` / `CANON_<FORM>_EXACT` feed
//! `lower.rs`'s special-form dispatch (cml#9 Finding 1): each pair lists
//! every Canon-registered spelling of one dispatch form (quote/cond/lambda/
//! define/defmacro) across every language the registry admits, so a source
//! written in Ukrainian or Sanskrit is recognized as that special form the
//! same as the English spelling is.
include!(concat!(env!("OUT_DIR"), "/canon_spellings.rs"));

/// True if `name` is the given Canon dispatch form's surface in any
/// registered language: uppercase-folded for `en`/`sym` surfaces (matching
/// ordinary symbol case-insensitivity), exact for `uk`/`sa` surfaces.
pub fn is_canon_form(name: &str, upper: &[&str], exact: &[&str]) -> bool {
    upper.contains(&name.to_uppercase().as_str()) || exact.contains(&name)
}
