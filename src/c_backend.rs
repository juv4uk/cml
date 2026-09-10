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
//! shared idea), and the first contract-2.1 slice: builtins bootstrapped as
//! ordinary callable values, higher-order use, lexical shadowing, canonical
//! `#<builtin name>` printing, and named non-callable/arity failures.  The C
//! path receives `lower_program_with_first_class_builtins`; fpga-lisp keeps
//! the contract-2.0 `Ir::Prim` path until it independently implements 2.1.
//!
//! The runtime is a small tagged-union `Value` with a mutable-cons alist
//! for environments -- the same conceptual model `compiler.rs` uses on
//! fpga-lisp (env is an alist chain; a name's binding is looked up by
//! walking it), just implemented directly as C structs instead of tagged
//! 32-bit words on a heap array.

use crate::ir::{BufferLiteral, Ir, Params, PrimOp, Quoted};
use std::fmt;

/// Errors that can occur during C code generation.
#[derive(Debug, Clone)]
pub enum CompileError {
    /// A `def` form appeared in a non-top-level position.
    NestedDef,
    /// Floating buffers are not yet represented by this scalar C runtime.
    UnsupportedTypedBuffer,
    /// An IR variant that this backend does not yet support.
    UnsupportedVariant(&'static str),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::NestedDef => write!(
                f,
                "def is only supported at the top level of a program, not nested"
            ),
            CompileError::UnsupportedTypedBuffer => write!(
                f,
                "unsupported typed numeric buffer in C backend: use a compute backend"
            ),
            CompileError::UnsupportedVariant(variant) => {
                write!(f, "unsupported IR variant in C backend: {variant}")
            }
        }
    }
}

impl std::error::Error for CompileError {}

pub struct CBackend {
    functions: Vec<String>,
    fn_counter: usize,
}

const RUNTIME: &str = r##"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>

typedef struct Value Value;
typedef enum { TAG_NIL, TAG_INT, TAG_SYM, TAG_CONS, TAG_I32_BUFFER, TAG_CLOSURE, TAG_BUILTIN, TAG_RATIONAL } Tag;
struct Value {
    Tag tag;
    union {
        long i;
        const char *sym;
        struct { Value *car; Value *cdr; } cons;
        struct { int *data; size_t len; } i32_buffer;
        struct { Value *(*fn)(Value *args, Value *env); Value *env; } closure;
        struct { const char *name; Value *(*fn)(Value *args, Value *env); } builtin;
        struct { long num; long den; } rat;
    } u;
};

static Value NIL_V = { TAG_NIL, { .i = 0 } };
/* Canonical `t` as an ordinary Symbol, not a manufactured Tag::True
 * primitive -- 2026-09-04, applying the owner's paradigm (substrates
 * witness my-lisp's semantics, never invent their own; see
 * docs/language-core-axioms.md's G1/G6/G7 boundary note in my-lisp).
 * Same fix shape already proven in fpga-lisp's RTL and wsm-my-lisp's
 * asm nucleus: t goes through the exact same representation any other
 * interned symbol already uses. Every existing `&TRUE_V` call site is
 * left untouched -- only what TRUE_V itself is changes. */
static Value TRUE_V = { TAG_SYM, { .sym = "T" } };
static Value *global_env = &NIL_V;

static void runtime_error(const char *kind, const char *detail);
static void *checked_malloc(size_t bytes) {
    void *memory = malloc(bytes == 0 ? 1 : bytes);
    if (memory == NULL) runtime_error("OutOfMemory", "C backend heap allocation");
    return memory;
}

static Value *mk_int(long n) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_INT; v->u.i = n; return v; }
static Value *mk_sym(const char *s) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_SYM; v->u.sym = s; return v; }
static Value *mk_cons(Value *a, Value *b) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_CONS; v->u.cons.car = a; v->u.cons.cdr = b; return v; }
static Value *mk_i32_buffer(const int *data, size_t len) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_I32_BUFFER; v->u.i32_buffer.data = checked_malloc(len * sizeof(int)); v->u.i32_buffer.len = len; memcpy(v->u.i32_buffer.data, data, len * sizeof(int)); return v; }
static Value *mk_closure(Value *(*fn)(Value*, Value*), Value *env) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_CLOSURE; v->u.closure.fn = fn; v->u.closure.env = env; return v; }
static Value *mk_builtin(const char *name, Value *(*fn)(Value*, Value*)) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_BUILTIN; v->u.builtin.name = name; v->u.builtin.fn = fn; return v; }

static long rational_gcd(long a, long b) {
    if (a < 0) a = -a;
    if (b < 0) b = -b;
    while (b != 0) { long t = a % b; a = b; b = t; }
    return a;
}

static long rational_checked_mul(long a, long b) {
    // Contract 3.0 named kind: NumericOverflow
    if (a > 0) {
        if (b > 0) { if (a > LONG_MAX / b) runtime_error("NumericOverflow", "rational numerator/denominator overflow"); }
        else if (b < 0) { if (b < LONG_MIN / a) runtime_error("NumericOverflow", "rational numerator/denominator overflow"); }
    } else if (a < 0) {
        if (b > 0) { if (a < LONG_MIN / b) runtime_error("NumericOverflow", "rational numerator/denominator overflow"); }
        else if (b < 0) { if (a != 0 && b < LONG_MAX / a) runtime_error("NumericOverflow", "rational numerator/denominator overflow"); }
    }
    return a * b;
}

// Build a normalized rational: denominator normalized positive (a negative
// value lives in the numerator), and both divided by their gcd. Values that
// reduce to an integer are not collapsed here -- they stay TAG_RATIONAL so
// the printed form is stable and the exact denominator is preserved -- but a
// zero denominator is a hard Type error, matching the WSM exact-arithmetic
// contract.
static Value *mk_rational(long num, long den) {
    if (den == 0) runtime_error("Type", "rational denominator is zero");
    if (den < 0) { num = -num; den = -den; }
    long g = rational_gcd(num, den);
    if (g != 0) { num /= g; den /= g; }
    Value *v = checked_malloc(sizeof(Value));
    v->tag = TAG_RATIONAL;
    v->u.rat.num = num;
    v->u.rat.den = den;
    return v;
}

static int rat_is_integer(Value *v) { return v->tag == TAG_RATIONAL && v->u.rat.den == 1; }

static Value *v_rat_add(Value *a, Value *b) {
    long n = rational_checked_mul(a->u.rat.num, b->u.rat.den) + rational_checked_mul(b->u.rat.num, a->u.rat.den);
    long d = rational_checked_mul(a->u.rat.den, b->u.rat.den);
    return mk_rational(n, d);
}
static Value *v_rat_sub(Value *a, Value *b) {
    long n = rational_checked_mul(a->u.rat.num, b->u.rat.den) - rational_checked_mul(b->u.rat.num, a->u.rat.den);
    long d = rational_checked_mul(a->u.rat.den, b->u.rat.den);
    return mk_rational(n, d);
}
static Value *v_rat_mul(Value *a, Value *b) {
    long n = rational_checked_mul(a->u.rat.num, b->u.rat.num);
    long d = rational_checked_mul(a->u.rat.den, b->u.rat.den);
    return mk_rational(n, d);
}
static Value *v_rat_div(Value *a, Value *b) {
    if (b->u.rat.num == 0) runtime_error("DivisionByZero", "rational division by zero");
    long n = rational_checked_mul(a->u.rat.num, b->u.rat.den);
    long d = rational_checked_mul(a->u.rat.den, b->u.rat.num);
    return mk_rational(n, d);
}

static Value *v_car(Value *v) { return v->u.cons.car; }
static Value *v_cdr(Value *v) { return v->u.cons.cdr; }
static int is_atom(Value *v) { return v->tag != TAG_CONS; }
static int truthy(Value *v) { return v->tag != TAG_NIL; }

static Value *v_eq(Value *a, Value *b) {
    if (a->tag != b->tag) return &NIL_V;
    switch (a->tag) {
        case TAG_NIL: return &TRUE_V;
        case TAG_INT: return a->u.i == b->u.i ? &TRUE_V : &NIL_V;
        case TAG_RATIONAL: return rational_checked_mul(a->u.rat.num, b->u.rat.den) == rational_checked_mul(b->u.rat.num, a->u.rat.den) ? &TRUE_V : &NIL_V;
        case TAG_SYM: return strcmp(a->u.sym, b->u.sym) == 0 ? &TRUE_V : &NIL_V;
        default: return a == b ? &TRUE_V : &NIL_V;
    }
}

static int v_equal_p(Value *a, Value *b) {
    if (a->tag != b->tag) return 0;
    switch (a->tag) {
        case TAG_NIL: return 1;
        case TAG_INT: return a->u.i == b->u.i;
        case TAG_RATIONAL: return rational_checked_mul(a->u.rat.num, b->u.rat.den) == rational_checked_mul(b->u.rat.num, a->u.rat.den);
        case TAG_SYM: return strcmp(a->u.sym, b->u.sym) == 0;
        case TAG_CONS: return v_equal_p(a->u.cons.car, b->u.cons.car) && v_equal_p(a->u.cons.cdr, b->u.cons.cdr);
        case TAG_I32_BUFFER:
            if (a->u.i32_buffer.len != b->u.i32_buffer.len) return 0;
            for (size_t i = 0; i < a->u.i32_buffer.len; i++) if (a->u.i32_buffer.data[i] != b->u.i32_buffer.data[i]) return 0;
            return 1;
        default: return a == b;
    }
}

// If either operand is rational, compute exactly using cross-multiplication;
// otherwise use plain fixnum traces, preserving the existing fast path.
static Value *to_rational(Value *v) {
    if (v->tag == TAG_RATIONAL) return v;
    if (v->tag == TAG_INT) return mk_rational(v->u.i, 1);
    return NULL;
}
static long checked_long_add(long a, long b) {
    if ((b > 0 && a > LONG_MAX - b) || (b < 0 && a < LONG_MIN - b))
        runtime_error("NumericOverflow", "integer addition overflow");
    return a + b;
}
static long checked_long_sub(long a, long b) {
    if ((b < 0 && a > LONG_MAX + b) || (b > 0 && a < LONG_MIN + b))
        runtime_error("NumericOverflow", "integer subtraction overflow");
    return a - b;
}
static long checked_long_mul(long a, long b) {
    if (a > 0) {
        if (b > 0) { if (a > LONG_MAX / b) runtime_error("NumericOverflow", "integer multiplication overflow"); }
        else if (b < 0) { if (b < LONG_MIN / a) runtime_error("NumericOverflow", "integer multiplication overflow"); }
    } else if (a < 0) {
        if (b > 0) { if (a < LONG_MIN / b) runtime_error("NumericOverflow", "integer multiplication overflow"); }
        else if (b < 0) { if (a != 0 && b < LONG_MAX / a) runtime_error("NumericOverflow", "integer multiplication overflow"); }
    }
    return a * b;
}
static Value *v_add(Value *a, Value *b) {
    if (a->tag == TAG_RATIONAL || b->tag == TAG_RATIONAL)
        return v_rat_add(to_rational(a), to_rational(b));
    return mk_int(checked_long_add(a->u.i, b->u.i));
}
static Value *v_sub(Value *a, Value *b) {
    if (a->tag == TAG_RATIONAL || b->tag == TAG_RATIONAL)
        return v_rat_sub(to_rational(a), to_rational(b));
    return mk_int(checked_long_sub(a->u.i, b->u.i));
}

static void runtime_error(const char *kind, const char *detail) {
    fprintf(stderr, "%s: %s\n", kind, detail);
    exit(1);
}

static int list_length(Value *args) {
    int length = 0;
    while (args->tag == TAG_CONS) { length++; args = v_cdr(args); }
    if (args->tag != TAG_NIL) runtime_error("Type", "call arguments must form a proper list");
    return length;
}

static void require_arity(Value *args, int expected, const char *name) {
    if (list_length(args) != expected) runtime_error("Arity", name);
}

static void require_min_arity(Value *args, int minimum, const char *name) {
    if (list_length(args) < minimum) runtime_error("Arity", name);
}

static void require_tag(Value *value, Tag expected, const char *name) {
    if (value->tag != expected) runtime_error("Type", name);
}

static Value *arg_at(Value *args, int index) {
    while (index-- > 0) args = v_cdr(args);
    return v_car(args);
}

static void require_number(Value *value, const char *name) {
    if (value->tag != TAG_INT && value->tag != TAG_RATIONAL) runtime_error("Type", name);
}

static Value *builtin_add(Value *args, Value *env) { (void)env; require_arity(args, 2, "+"); require_number(arg_at(args, 0), "+"); require_number(arg_at(args, 1), "+"); return v_add(arg_at(args, 0), arg_at(args, 1)); }
static Value *builtin_subtract(Value *args, Value *env) {
    (void)env;
    require_min_arity(args, 1, "-");
    Value *result = arg_at(args, 0);
    require_number(result, "-");
    args = v_cdr(args);
    if (args->tag == TAG_NIL) {
        if (result->tag == TAG_RATIONAL) return mk_rational(-result->u.rat.num, result->u.rat.den);
        return mk_int(-result->u.i);
    }
    while (args->tag == TAG_CONS) {
        Value *operand = v_car(args);
        require_number(operand, "-");
        result = v_sub(result, operand);
        args = v_cdr(args);
    }
    return result;
}
static Value *builtin_mul(Value *args, Value *env) {
    (void)env;
    require_min_arity(args, 2, "*");
    Value *result = arg_at(args, 0);
    require_number(result, "*");
    args = v_cdr(args);
    while (args->tag == TAG_CONS) {
        Value *operand = v_car(args);
        require_number(operand, "*");
        if (result->tag == TAG_RATIONAL || operand->tag == TAG_RATIONAL)
            result = v_rat_mul(to_rational(result), to_rational(operand));
        else result = mk_int(checked_long_mul(result->u.i, operand->u.i));
        args = v_cdr(args);
    }
    return result;
}
static Value *builtin_div(Value *args, Value *env) {
    (void)env;
    require_min_arity(args, 2, "/");
    Value *result = arg_at(args, 0);
    require_number(result, "/");
    args = v_cdr(args);
    while (args->tag == TAG_CONS) {
        Value *operand = v_car(args);
        require_number(operand, "/");
        result = v_rat_div(to_rational(result), to_rational(operand));
        args = v_cdr(args);
    }
    return result;
}
static Value *builtin_cons(Value *args, Value *env) { (void)env; require_arity(args, 2, "cons"); return mk_cons(arg_at(args, 0), arg_at(args, 1)); }
static Value *builtin_car(Value *args, Value *env) { (void)env; require_arity(args, 1, "car"); require_tag(arg_at(args, 0), TAG_CONS, "car"); return v_car(arg_at(args, 0)); }
static Value *builtin_cdr(Value *args, Value *env) { (void)env; require_arity(args, 1, "cdr"); require_tag(arg_at(args, 0), TAG_CONS, "cdr"); return v_cdr(arg_at(args, 0)); }
static Value *builtin_eq(Value *args, Value *env) {
    (void)env;
    require_arity(args, 2, "eq");
    Value *left = arg_at(args, 0);
    Value *right = arg_at(args, 1);
    if (left->tag == TAG_CONS || right->tag == TAG_CONS) runtime_error("Type", "eq");
    return v_eq(left, right);
}
static Value *builtin_atom(Value *args, Value *env) { (void)env; require_arity(args, 1, "atom"); return is_atom(arg_at(args, 0)) ? &TRUE_V : &NIL_V; }
static Value *builtin_equal_p(Value *args, Value *env) { (void)env; require_arity(args, 2, "equal?"); return v_equal_p(arg_at(args, 0), arg_at(args, 1)) ? &TRUE_V : &NIL_V; }

static Value *v_apply(Value *callable, Value *args) {
    if (callable->tag == TAG_CLOSURE) return callable->u.closure.fn(args, callable->u.closure.env);
    if (callable->tag == TAG_BUILTIN) return callable->u.builtin.fn(args, &NIL_V);
    runtime_error("NotCallable", "attempted to call a non-callable value");
    return &NIL_V;
}

static Value *v_map_i32_buffer(Value *callable, Value *buffer) {
    require_tag(buffer, TAG_I32_BUFFER, "numeric-buffer-map");
    size_t len = buffer->u.i32_buffer.len;
    int *out = checked_malloc(len * sizeof(int));
    for (size_t i = 0; i < len; i++) {
        Value *mapped = v_apply(callable, mk_cons(mk_int(buffer->u.i32_buffer.data[i]), &NIL_V));
        require_tag(mapped, TAG_INT, "numeric-buffer-map");
        if (mapped->u.i < INT_MIN || mapped->u.i > INT_MAX) {
            runtime_error("NumericOverflow", "numeric-buffer-map");
        }
        out[i] = (int)mapped->u.i;
    }
    Value *result = mk_i32_buffer(out, len);
    free(out);
    return result;
}

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
    runtime_error("UnknownSymbol", name);
    return &NIL_V;
}

static void bind_global(const char *name, Value *value) {
    global_env = mk_cons(mk_cons(mk_sym(name), value), global_env);
}

static void bootstrap_builtins(void) {
    bind_global("+", mk_builtin("+", builtin_add));
    bind_global("-", mk_builtin("-", builtin_subtract));
    bind_global("*", mk_builtin("*", builtin_mul));
    bind_global("/", mk_builtin("/", builtin_div));
    bind_global("CONS", mk_builtin("cons", builtin_cons));
    bind_global("CAR", mk_builtin("car", builtin_car));
    bind_global("CDR", mk_builtin("cdr", builtin_cdr));
    bind_global("EQ", mk_builtin("eq", builtin_eq));
    bind_global("ATOM", mk_builtin("atom", builtin_atom));
    bind_global("EQUAL?", mk_builtin("equal?", builtin_equal_p));
}

// Standard Lisp list printing (`(a b c)`, `(a b . c)` for a genuine
// dotted tail), not a raw nested-dotted-pair dump -- lets a compiled
// program's printed output be compared directly against my-lisp's own
// printer / tests/fixtures/conformance.my's `expected` field.
static void print_value(Value *v) {
    switch (v->tag) {
        case TAG_NIL: printf("()"); break;
        case TAG_INT: printf("%ld", v->u.i); break;
        case TAG_RATIONAL:
            // Same shape as my-lisp's Value::Rational Display: `n` when the
            // reduced denominator is 1, else `n/d`. Keeps a compiled
            // program's output directly comparable against the oracle.
            if (v->u.rat.den == 1) printf("%ld", v->u.rat.num);
            else printf("%ld/%ld", v->u.rat.num, v->u.rat.den);
            break;
        case TAG_SYM: printf("%s", v->u.sym); break;
        case TAG_CLOSURE: printf("<closure>"); break;
        case TAG_BUILTIN: printf("#<builtin %s>", v->u.builtin.name); break;
        case TAG_I32_BUFFER:
            printf("#i32(");
            for (size_t i = 0; i < v->u.i32_buffer.len; i++) { if (i) printf(" "); printf("%d", v->u.i32_buffer.data[i]); }
            printf(")");
            break;
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
"##;

impl CBackend {
    pub fn new() -> Self {
        CBackend {
            functions: Vec::new(),
            fn_counter: 0,
        }
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
        // my-lisp empty-program semantics (TASK-001)
        let mut printed_result = false;
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
                    printed_result = true;
                }
            }
        }

        if !printed_result {
            main_body.push_str("    { print_value(&NIL_V); printf(\"\\n\"); }\\n");
        }

        Ok(format!(
            "{RUNTIME}\n{}\n\nint main(void) {{\n    bootstrap_builtins();\n{}    return 0;\n}}\n",
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
            Ir::Float(_) => Err(CompileError::UnsupportedVariant("Float")),
            Ir::Rational(num, den) => Ok(format!("mk_rational({num}, {den})")),
            Ir::String(_) => Err(CompileError::UnsupportedVariant("String")),
            Ir::Buffer(BufferLiteral::I32(values)) => {
                let data = values
                    .iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                Ok(format!(
                    "mk_i32_buffer((int[]){{{data}}}, {})",
                    values.len()
                ))
            }
            Ir::Buffer(BufferLiteral::F32(_)) => Err(CompileError::UnsupportedTypedBuffer),
            Ir::Nil => Ok("(&NIL_V)".to_string()),
            Ir::True => Ok("(&TRUE_V)".to_string()),
            Ir::Var(name) => Ok(format!("env_lookup({env}, \"{name}\")")),
            Ir::Builtin(_) => Err(CompileError::UnsupportedVariant("Builtin")),
            Ir::Quote(q) => self.compile_quoted(q),
            Ir::Lambda { params, body } => self.compile_lambda(params, body, env),
            Ir::App { func, args } => self.compile_app(func, args, env),
            Ir::Cond { branches } => self.compile_cond(branches, env),
            Ir::Let { bindings, body } => {
                let params = Params::Fixed(bindings.iter().map(|(n, _)| n.clone()).collect());
                let args: Vec<Ir> = bindings.iter().map(|(_, v)| v.clone()).collect();
                let lambda = Ir::Lambda {
                    params,
                    body: Box::new((**body).clone()),
                };
                self.compile_app(&lambda, &args, env)
            }
            Ir::Def { .. } => Err(CompileError::NestedDef),
            Ir::TailSelfCall { .. } => Err(CompileError::UnsupportedVariant("TailSelfCall")),
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
            PrimOp::Sub => Ok(format!(
                "v_sub({}, {})",
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
            PrimOp::Atom => Ok(format!(
                "(is_atom({}) ? &TRUE_V : &NIL_V)",
                self.compile_expr(&args[0], env)?
            )),
            PrimOp::EqualP => Ok(format!(
                "(v_equal_p({}, {}) ? &TRUE_V : &NIL_V)",
                self.compile_expr(&args[0], env)?,
                self.compile_expr(&args[1], env)?
            )),
        }
    }

    fn compile_quoted(&mut self, q: &Quoted) -> Result<String, CompileError> {
        match q {
            Quoted::Int(n) => Ok(format!("mk_int({n})")),
            Quoted::Float(_) => Err(CompileError::UnsupportedVariant("Quoted::Float")),
            Quoted::Rational(num, den) => Ok(format!("mk_rational({num}, {den})")),
            Quoted::Sym(s) | Quoted::Str(s) => Ok(format!("mk_sym(\"{s}\")")),
            Quoted::Nil => Ok("(&NIL_V)".to_string()),
            Quoted::List(items) => {
                let mut acc = "(&NIL_V)".to_string();
                for item in items.iter().rev() {
                    let item_expr = self.compile_quoted(item)?;
                    acc = format!("mk_cons({item_expr}, {acc})");
                }
                Ok(acc)
            }
            Quoted::DottedList(items, tail) => {
                let mut acc = self.compile_quoted(tail)?;
                for item in items.iter().rev() {
                    let item_expr = self.compile_quoted(item)?;
                    acc = format!("mk_cons({item_expr}, {acc})");
                }
                Ok(acc)
            }
        }
    }

    /// A lambda becomes its own top-level C function (`fn_ptr(args, env)`,
    /// `args` a cons-list of the actual arguments) plus a closure value
    /// pairing that function pointer with the *current* env -- captured
    /// at the point the closure is created, same as `compiler.rs`'s
    /// `CONS closure_reg label env_reg`.
    fn compile_lambda(
        &mut self,
        params: &Params,
        body: &Ir,
        env: &str,
    ) -> Result<String, CompileError> {
        let fn_name = self.next_fn_name();

        let mut fn_body = String::new();
        fn_body.push_str("    Value *args_cursor = args;\n");
        match params {
            Params::Fixed(names) => {
                fn_body.push_str(&format!(
                    "    require_arity(args, {}, \"lambda\");\n",
                    names.len()
                ));
                for name in names {
                    fn_body.push_str(&format!(
                        "    env = mk_cons(mk_cons(mk_sym(\"{name}\"), v_car(args_cursor)), env);\n    args_cursor = v_cdr(args_cursor);\n"
                    ));
                }
            }
            Params::Variadic { fixed, rest } => {
                fn_body.push_str(&format!(
                    "    require_min_arity(args, {}, \"lambda\");\n",
                    fixed.len()
                ));
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
        if let Ir::Var(name) = func {
            if name == "NUMERIC-BUFFER-MAP" && args.len() == 2 {
                let function = self.compile_expr(&args[0], env)?;
                let buffer = self.compile_expr(&args[1], env)?;
                return Ok(format!("v_map_i32_buffer({function}, {buffer})"));
            }
        }
        let func_expr = self.compile_expr(func, env)?;
        let mut args_list = "(&NIL_V)".to_string();
        for arg in args.iter().rev() {
            let arg_expr = self.compile_expr(arg, env)?;
            args_list = format!("mk_cons({arg_expr}, {args_list})");
        }
        Ok(format!(
            "({{ Value *_f = {func_expr}; v_apply(_f, ({args_list})); }})"
        ))
    }

    fn compile_cond(&mut self, branches: &[(Ir, Ir)], env: &str) -> Result<String, CompileError> {
        let mut out = String::from("({ Value *_c;");
        let mut first = true;
        for (test, body) in branches {
            let test_expr = self.compile_expr(test, env)?;
            let body_expr = self.compile_expr(body, env)?;
            if first {
                out.push_str(&format!(
                    " if (truthy({test_expr})) {{ _c = {body_expr}; }}"
                ));
                first = false;
            } else {
                out.push_str(&format!(
                    " else if (truthy({test_expr})) {{ _c = {body_expr}; }}"
                ));
            }
        }
        out.push_str(" else { _c = &NIL_V; } _c; })");
        Ok(out)
    }
}
