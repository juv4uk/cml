//! A second consumer of `ir::Ir` (docs/heterogeneous-backends.md step 2):
//! a minimal C emitter, proving the IR extracted in `ir.rs`/`lower.rs` is
//! really backend-neutral and not just fpga-lisp-shaped by accident.
//!
//! Deliberately scoped down from full language coverage -- this is the
//! first C backend increment, not a claim of parity with `compiler.rs`.
//! Supported: integers, `nil`/`t`, variables, `quote` of integers/symbols/
//! lists/dotted lists (CML-C-BACKEND-QUOTED-LISTS), `lambda` with fixed,
//! variadic (`(a b . rest)`), and bare-symbol (`args`) param lists
//! (CML-C-BACKEND-VARIADIC), application, `cond`, `let`
//! (CML-C-BACKEND-LET, derived via an immediately-applied lambda, same
//! technique `compiler.rs` uses), structural `equal?` (`v_equal_p`,
//! recursive -- not just pointer equality), top-level `def` (including
//! self-recursive, via the same letrec-placeholder-plus-backpatch
//! technique `compiler.rs`'s `compile_def` uses on fpga-lisp -- see that
//! function's doc comment and `docs/abi.md`'s `def` section for the
//! shared idea).
//!
//! The runtime is a small tagged-union `Value` with a mutable-cons alist
//! for environments -- the same conceptual model `compiler.rs` uses on
//! fpga-lisp (env is an alist chain; a name's binding is looked up by
//! walking it), just implemented directly as C structs instead of tagged
//! 32-bit words on a heap array.

use crate::ir::{Ir, Params, PrimOp, Quoted};
use std::fmt;

/// Errors that can occur during C code generation.
#[derive(Debug, Clone)]
pub enum CompileError {
    /// A `def` form appeared in a non-top-level position.
    NestedDef,
    /// An IR node that this backend does not yet support.
    Unsupported(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::NestedDef => write!(
                f, "def is only supported at the top level of a program, not nested"
            ),
            CompileError::Unsupported(node) => write!(
                f, "unsupported IR node in C backend: {node}"
            ),
        }
    }
}

impl std::error::Error for CompileError {}

pub struct CBackend {
    functions: Vec<String>,
    fn_counter: usize,
}

const RUNTIME: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Value Value;
typedef enum { TAG_NIL, TAG_TRUE, TAG_INT, TAG_SYM, TAG_CONS, TAG_CLOSURE } Tag;
struct Value {
    Tag tag;
    union {
        long i;
        const char *sym;
        struct { Value *car; Value *cdr; } cons;
        struct { Value *(*fn)(Value *args, Value *env); Value *env; } closure;
    } u;
};

static Value NIL_V = { TAG_NIL, { .i = 0 } };
static Value TRUE_V = { TAG_TRUE, { .i = 0 } };
static Value *global_env = &NIL_V;

static Value *mk_int(long n) { Value *v = malloc(sizeof(Value)); v->tag = TAG_INT; v->u.i = n; return v; }
static Value *mk_sym(const char *s) { Value *v = malloc(sizeof(Value)); v->tag = TAG_SYM; v->u.sym = s; return v; }
static Value *mk_cons(Value *a, Value *b) { Value *v = malloc(sizeof(Value)); v->tag = TAG_CONS; v->u.cons.car = a; v->u.cons.cdr = b; return v; }
static Value *mk_closure(Value *(*fn)(Value*, Value*), Value *env) { Value *v = malloc(sizeof(Value)); v->tag = TAG_CLOSURE; v->u.closure.fn = fn; v->u.closure.env = env; return v; }

static Value *v_car(Value *v) { return v->u.cons.car; }
static Value *v_cdr(Value *v) { return v->u.cons.cdr; }
static int is_atom(Value *v) { return v->tag != TAG_CONS; }
static int truthy(Value *v) { return v->tag != TAG_NIL; }

static Value *v_eq(Value *a, Value *b) {
    if (a->tag != b->tag) return &NIL_V;
    switch (a->tag) {
        case TAG_NIL: case TAG_TRUE: return &TRUE_V;
        case TAG_INT: return a->u.i == b->u.i ? &TRUE_V : &NIL_V;
        case TAG_SYM: return strcmp(a->u.sym, b->u.sym) == 0 ? &TRUE_V : &NIL_V;
        default: return a == b ? &TRUE_V : &NIL_V;
    }
}

static int v_equal_p(Value *a, Value *b) {
    if (a->tag != b->tag) return 0;
    switch (a->tag) {
        case TAG_NIL: case TAG_TRUE: return 1;
        case TAG_INT: return a->u.i == b->u.i;
        case TAG_SYM: return strcmp(a->u.sym, b->u.sym) == 0;
        case TAG_CONS: return v_equal_p(a->u.cons.car, b->u.cons.car) && v_equal_p(a->u.cons.cdr, b->u.cons.cdr);
        default: return a == b;
    }
}

static Value *v_add(Value *a, Value *b) { return mk_int(a->u.i + b->u.i); }

// Environment lookup: env is an alist chain, ((sym . val) . rest), same
// shape as compiler.rs's cml_lookup on fpga-lisp.
static Value *env_lookup(Value *env, const char *name) {
    while (env->tag == TAG_CONS) {
        Value *pair = env->u.cons.car;
        if (strcmp(pair->u.cons.car->u.sym, name) == 0) {
            return pair->u.cons.cdr;
        }
        env = env->u.cons.cdr;
    }
    fprintf(stderr, "unbound variable: %s\n", name);
    exit(1);
}

// Standard Lisp list printing (`(a b c)`, `(a b . c)` for a genuine
// dotted tail), not a raw nested-dotted-pair dump -- lets a compiled
// program's printed output be compared directly against my-lisp's own
// printer / tests/fixtures/conformance.my's `expected` field.
static void print_value(Value *v) {
    switch (v->tag) {
        case TAG_NIL: printf("()"); break;
        case TAG_TRUE: printf("t"); break;
        case TAG_INT: printf("%ld", v->u.i); break;
        case TAG_SYM: printf("%s", v->u.sym); break;
        case TAG_CLOSURE: printf("<closure>"); break;
        case TAG_CONS: {
            printf("(");
            Value *cur = v;
            int first = 1;
            while (cur->tag == TAG_CONS) {
                if (!first) printf(" ");
                print_value(cur->u.cons.car);
                first = 0;
                cur = cur->u.cons.cdr;
            }
            if (cur->tag != TAG_NIL) {
                printf(" . ");
                print_value(cur);
            }
            printf(")");
            break;
        }
    }
}
"#;

impl CBackend {
    pub fn new() -> Self {
        CBackend { functions: Vec::new(), fn_counter: 0 }
    }

    fn next_fn_name(&mut self) -> String {
        self.fn_counter += 1;
        format!("cml_lambda_{}", self.fn_counter)
    }

    /// Compiles a whole program into a self-contained C source file. Every
    /// top-level `Ir::Def` becomes a global binding (self-recursive defs
    /// via the letrec-placeholder-plus-backpatch pattern, mirroring
    /// `compiler.rs`'s `compile_def`); the last non-`Def` top-level form's
    /// value is printed. Program shape assumed: zero or more `Def`s
    /// followed by exactly one expression -- the same shape every
    /// `evidence/`-worthy `cml` fixture in this repo already has.
    pub fn compile_program(&mut self, program: &[Ir]) -> Result<String, CompileError> {
        let mut main_body = String::new();
        for ir in program {
            match ir {
                Ir::Def { name, value } => {
                    main_body.push_str(&self.compile_def(name, value)?);
                }
                other => {
                    let expr = self.compile_expr(other, "global_env")?;
                    main_body.push_str(&format!(
                        "    {{ Value *result = {expr}; print_value(result); printf(\"\\n\"); }}\n"
                    ));
                }
            }
        }

        Ok(format!(
            "{RUNTIME}\n{}\n\nint main(void) {{\n{}    return 0;\n}}\n",
            self.functions.join("\n"),
            main_body,
        ))
    }

    /// `(def name value)`: extends `global_env` with a placeholder pair
    /// `(name . nil)` *before* compiling `value`, so a `value` that's a
    /// lambda captures the extended env and can look itself up by name;
    /// then backpatches the placeholder's cdr in place -- the same
    /// letrec-placeholder-plus-SETCDR idea `compiler.rs`'s `compile_def`
    /// uses on fpga-lisp, here as a literal C struct-field mutation.
    fn compile_def(&mut self, name: &str, value: &Ir) -> Result<String, CompileError> {
        let mut out = String::new();
        out.push_str(&format!(
            "    Value *ph_{name} = mk_cons(mk_sym(\"{name}\"), &NIL_V);\n"
        ));
        out.push_str(&format!(
            "    global_env = mk_cons(ph_{name}, global_env);\n"
        ));
        let value_expr = self.compile_expr(value, "global_env")?;
        out.push_str(&format!("    ph_{name}->u.cons.cdr = {value_expr};\n"));
        Ok(out)
    }

    fn compile_expr(&mut self, ir: &Ir, env: &str) -> Result<String, CompileError> {
        match ir {
            Ir::Int(n) => Ok(format!("mk_int({n})")),
            Ir::Nil => Ok("(&NIL_V)".to_string()),
            Ir::True => Ok("(&TRUE_V)".to_string()),
            Ir::Var(name) => Ok(format!("env_lookup({env}, \"{name}\")")),
            Ir::Quote(q) => Ok(self.compile_quoted(q)),
            Ir::Lambda { params, body } => self.compile_lambda(params, body, env),
            Ir::App { func, args } => self.compile_app(func, args, env),
            Ir::Cond { branches } => self.compile_cond(branches, env),
            Ir::Let { bindings, body } => {
                let params = Params::Fixed(bindings.iter().map(|(n, _)| n.clone()).collect());
                let args: Vec<Ir> = bindings.iter().map(|(_, v)| v.clone()).collect();
                let lambda = Ir::Lambda { params, body: Box::new((**body).clone()) };
                self.compile_app(&lambda, &args, env)
            }
            Ir::Def { .. } => Err(CompileError::NestedDef),
            Ir::Prim { op, args } => self.compile_prim(*op, args, env),
        }
    }

    fn compile_prim(&mut self, op: PrimOp, args: &[Ir], env: &str) -> Result<String, CompileError> {
        match op {
            PrimOp::Add => Ok(format!(
                "v_add({}, {})",
                self.compile_expr(&args[0], env)?,
                self.compile_expr(&args[1], env)?
            )),
            PrimOp::Cons => Ok(format!(
                "mk_cons({}, {})",
                self.compile_expr(&args[0], env)?,
                self.compile_expr(&args[1], env)?
            )),
            PrimOp::Car => Ok(format!("v_car({})", self.compile_expr(&args[0], env)?)),
            PrimOp::Cdr => Ok(format!("v_cdr({})", self.compile_expr(&args[0], env)?)),
            PrimOp::Eq => Ok(format!(
                "v_eq({}, {})",
                self.compile_expr(&args[0], env)?,
                self.compile_expr(&args[1], env)?
            )),
            PrimOp::Atom => Ok(format!("(is_atom({}) ? &TRUE_V : &NIL_V)", self.compile_expr(&args[0], env)?)),
            PrimOp::EqualP => Ok(format!(
                "(v_equal_p({}, {}) ? &TRUE_V : &NIL_V)",
                self.compile_expr(&args[0], env)?,
                self.compile_expr(&args[1], env)?
            )),
        }
    }

    fn compile_quoted(&mut self, q: &Quoted) -> String {
        match q {
            Quoted::Int(n) => format!("mk_int({n})"),
            Quoted::Sym(s) | Quoted::Str(s) => format!("mk_sym(\"{s}\")"),
            Quoted::Nil => "(&NIL_V)".to_string(),
            // Unlike compiler.rs's fpga-lisp path (which needs an explicit
            // R11-stack accumulator since it's emitting a flat instruction
            // stream), a C expression can just nest mk_cons calls directly
            // -- built tail-first, same right-to-left order.
            Quoted::List(items) => {
                let mut acc = "(&NIL_V)".to_string();
                for item in items.iter().rev() {
                    let item_expr = self.compile_quoted(item);
                    acc = format!("mk_cons({item_expr}, {acc})");
                }
                acc
            }
            Quoted::DottedList(items, tail) => {
                let mut acc = self.compile_quoted(tail);
                for item in items.iter().rev() {
                    let item_expr = self.compile_quoted(item);
                    acc = format!("mk_cons({item_expr}, {acc})");
                }
                acc
            }
        }
    }

    /// A lambda becomes its own top-level C function (`fn_ptr(args, env)`,
    /// `args` a cons-list of the actual arguments) plus a closure value
    /// pairing that function pointer with the *current* env -- captured
    /// at the point the closure is created, same as `compiler.rs`'s
    /// `CONS closure_reg label env_reg`.
    fn compile_lambda(&mut self, params: &Params, body: &Ir, env: &str) -> Result<String, CompileError> {
        let fn_name = self.next_fn_name();

        let mut fn_body = String::new();
        fn_body.push_str("    Value *args_cursor = args;\n");
        match params {
            Params::Fixed(names) => {
                for name in names {
                    fn_body.push_str(&format!(
                        "    env = mk_cons(mk_cons(mk_sym(\"{name}\"), v_car(args_cursor)), env);\n    args_cursor = v_cdr(args_cursor);\n"
                    ));
                }
            }
            Params::Variadic { fixed, rest } => {
                for name in fixed {
                    fn_body.push_str(&format!(
                        "    env = mk_cons(mk_cons(mk_sym(\"{name}\"), v_car(args_cursor)), env);\n    args_cursor = v_cdr(args_cursor);\n"
                    ));
                }
                fn_body.push_str(&format!(
                    "    env = mk_cons(mk_cons(mk_sym(\"{rest}\"), args_cursor), env);\n"
                ));
            }
            Params::AllRest(rest) => {
                fn_body.push_str(&format!(
                    "    env = mk_cons(mk_cons(mk_sym(\"{rest}\"), args), env);\n"
                ));
            }
        }
        let body_expr = self.compile_expr(body, "env")?;
        fn_body.push_str(&format!("    return {body_expr};\n"));

        self.functions.push(format!(
            "static Value *{fn_name}(Value *args, Value *env) {{\n{fn_body}}}\n"
        ));

        Ok(format!("mk_closure({fn_name}, {env})"))
    }

    fn compile_app(&mut self, func: &Ir, args: &[Ir], env: &str) -> Result<String, CompileError> {
        let func_expr = self.compile_expr(func, env)?;
        let mut args_list = "(&NIL_V)".to_string();
        for arg in args.iter().rev() {
            let arg_expr = self.compile_expr(arg, env)?;
            args_list = format!("mk_cons({arg_expr}, {args_list})");
        }
        Ok(format!(
            "({{ Value *_f = {func_expr}; _f->u.closure.fn(({args_list}), _f->u.closure.env); }})"
        ))
    }

    fn compile_cond(&mut self, branches: &[(Ir, Ir)], env: &str) -> Result<String, CompileError> {
        let mut out = String::from("({ Value *_c;");
        let mut first = true;
        for (test, body) in branches {
            let test_expr = self.compile_expr(test, env)?;
            let body_expr = self.compile_expr(body, env)?;
            if first {
                out.push_str(&format!(" if (truthy({test_expr})) {{ _c = {body_expr}; }}"));
                first = false;
            } else {
                out.push_str(&format!(" else if (truthy({test_expr})) {{ _c = {body_expr}; }}"));
            }
        }
        out.push_str(" else { _c = &NIL_V; } _c; })");
        Ok(out)
    }
}
