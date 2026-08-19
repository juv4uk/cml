//! Backend-neutral intermediate representation.
//!
//! Step 1 of docs/heterogeneous-backends.md: "draw the backend boundary
//! inside cml" -- an IR the fpga-lisp emitter (and, later, a C emitter)
//! can both consume, so closures/env-chains/call structure become
//! explicit data here instead of being hardcoded directly into register
//! allocation the way `compiler.rs` does today (see `docs/abi.md`).
//!
//! `compiler.rs` (fpga-lisp) and `c_backend.rs` (C) both compile this same
//! `Ir` -- see `lower.rs` for the `ast::Expr -> Ir` step that feeds both,
//! and `main.rs` for the live `parse -> macro-expand -> lower -> backend`
//! pipeline.

/// A fully self-contained literal produced by `quote` -- data, never
/// executed. Kept separate from `Ir` itself because quoted data has no
/// binding structure (no `Var`, no `App`) to normalize.
#[derive(Debug, Clone, PartialEq)]
pub enum Quoted {
    Int(i64),
    Sym(String),
    Str(String),
    Nil,
    List(Vec<Quoted>),
    DottedList(Vec<Quoted>, Box<Quoted>),
}

/// The primitive operations `compiler.rs` currently lowers directly to
/// fpga-lisp opcodes (`compile_call`'s non-`_` arms). Kept as a closed
/// set here, mirroring what's actually implemented -- not aspirational.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimOp {
    Add,
    Cons,
    Car,
    Cdr,
    Eq,
    Atom,
    EqualP,
}

/// Fixed vs. variadic parameter lists, mirroring `compile_lambda`'s three
/// cases (`Expr::List`, `Expr::DottedList`, bare `Expr::Symbol`).
#[derive(Debug, Clone, PartialEq)]
pub enum Params {
    Fixed(Vec<String>),
    Variadic { fixed: Vec<String>, rest: String },
    AllRest(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ir {
    Int(i64),
    /// `nil` / `()` -- the empty list / false value.
    Nil,
    /// `t` -- the canonical true atom.
    True,
    /// A variable reference, resolved by the backend's own env mechanism
    /// (an alist walk on fpga-lisp today; a stack slot or register for a
    /// future C backend -- deliberately not specified here).
    Var(String),
    Quote(Quoted),
    Lambda {
        params: Params,
        body: Box<Ir>,
    },
    /// `(f a b ...)` where `f` is itself an expression (a symbol looked
    /// up as a user function, or a literal `(lambda ...)`).
    App {
        func: Box<Ir>,
        args: Vec<Ir>,
    },
    Cond {
        branches: Vec<(Ir, Ir)>,
    },
    /// `(let ((n v) ...) body)`; `compiler.rs` itself lowers this to an
    /// immediately-applied lambda (`compile_let`) rather than treating it
    /// as primitive, but it's common enough across backends to keep as
    /// its own IR node instead of re-deriving the lambda-application shape
    /// downstream of every backend.
    Let {
        bindings: Vec<(String, Ir)>,
        body: Box<Ir>,
    },
    /// `(def name value)` -- self-recursive via the letrec placeholder
    /// pattern on whichever backend implements it (see `docs/abi.md`'s
    /// `def` section for fpga-lisp's).
    Def {
        name: String,
        value: Box<Ir>,
    },
    Prim {
        op: PrimOp,
        args: Vec<Ir>,
    },
}
