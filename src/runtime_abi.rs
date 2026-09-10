//! COMPILER-02 — Runtime ABI v0 (C backend).
//!
//! Machine-readable description of the stable surface that generated C may
//! depend on. The ABI is extracted from `c_backend.rs`'s embedded RUNTIME;
//! this module does not re-implement the runtime — it names and tests the
//! contract so COMPILER-01/00 can treat runtime and emitter as separate
//! translation units later.
//!
//! Scope (v0): Value representation, constructors, predicates, call
//! convention, entry/bootstrap, structured failure kinds. Not a claim of
//! multi-backend ABI parity.

/// ABI revision. Bump only on breaking changes to the C surface.
pub const ABI_VERSION: (u32, u32) = (0, 1);

/// Tag discriminant names as emitted in RUNTIME.
pub const TAGS: &[&str] = &[
    "TAG_NIL",
    "TAG_INT",
    "TAG_SYM",
    "TAG_CONS",
    "TAG_I32_BUFFER",
    "TAG_CLOSURE",
    "TAG_BUILTIN",
    "TAG_RATIONAL",
];

/// Value constructors generated code may call.
pub const CONSTRUCTORS: &[&str] = &[
    "mk_int",
    "mk_sym",
    "mk_cons",
    "mk_i32_buffer",
    "mk_closure",
    "mk_builtin",
    "mk_rational",
];

/// Predicates / structural ops.
pub const PREDICATES: &[&str] = &[
    "v_car",
    "v_cdr",
    "is_atom",
    "truthy",
    "v_eq",
    "v_equal_p",
];

/// Arithmetic helpers (exact int + rational).
pub const ARITHMETIC: &[&str] = &[
    "v_add",
    "v_sub",
    "v_rat_add",
    "v_rat_sub",
    "v_rat_mul",
    "v_rat_div",
    "checked_long_add",
    "checked_long_sub",
    "checked_long_mul",
];

/// Call / arity / environment.
pub const CALL_ENV: &[&str] = &[
    "v_apply",
    "list_length",
    "require_arity",
    "require_min_arity",
    "require_tag",
    "require_number",
    "arg_at",
    "env_lookup",
    "bind_global",
    "bootstrap_builtins",
];

/// Contract 3.0-oriented named error kinds printed by `runtime_error`.
pub const ERROR_KINDS: &[&str] = &[
    "Arity",
    "Type",
    "UnknownSymbol",
    "OutOfMemory",
    "DivisionByZero",
    "NumericOverflow",
    "NotCallable",
];

/// Globals the generated `main` relies on.
pub const GLOBALS: &[&str] = &["NIL_V", "TRUE_V", "global_env"];

/// Entry contract: generated programs define `int main(void)`.
pub const ENTRY: &str = "main";

/// Call convention: closures and builtins take `(Value *args, Value *env)`
/// and return `Value *`. Args are a proper list.
pub const CALL_CONVENTION: &str = "Value *(*fn)(Value *args, Value *env)";

/// Every required ABI symbol must appear in emitted C for a trivial program.
pub fn required_symbols() -> impl Iterator<Item = &'static str> {
    TAGS.iter()
        .chain(CONSTRUCTORS.iter())
        .chain(PREDICATES.iter())
        .chain(ARITHMETIC.iter())
        .chain(CALL_ENV.iter())
        .chain(GLOBALS.iter())
        .copied()
}

/// True if `c_source` contains every required ABI symbol (substring check).
pub fn c_source_exports_abi_v0(c_source: &str) -> Result<(), Vec<&'static str>> {
    let missing: Vec<&'static str> = required_symbols()
        .filter(|sym| !c_source.contains(sym))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{emit_c, front_end_to_ir};

    #[test]
    fn abi_version_is_v0() {
        assert_eq!(ABI_VERSION, (0, 1));
    }

    #[test]
    fn trivial_emit_exports_runtime_abi_v0() {
        let ir = front_end_to_ir("(+ 1 2)").expect("front_end");
        let c = emit_c(&ir).expect("emit");
        c_source_exports_abi_v0(&c).unwrap_or_else(|missing| {
            panic!("RUNTIME missing ABI symbols: {missing:?}");
        });
        assert!(c.contains("int main(void)"));
        assert!(c.contains("bootstrap_builtins"));
        assert!(c.contains("runtime_error"));
        for kind in ERROR_KINDS {
            assert!(
                c.contains(&format!("\"{kind}\"")) || c.contains(kind),
                "error kind {kind} not present in RUNTIME"
            );
        }
    }
}
