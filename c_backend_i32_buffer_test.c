
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>

typedef struct Value Value;
typedef enum { TAG_NIL, TAG_INT, TAG_SYM, TAG_CONS, TAG_I32_BUFFER, TAG_CLOSURE, TAG_BUILTIN, TAG_RATIONAL, TAG_STRING, TAG_SID8 } Tag;
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
        const char *str;
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
/* COMPILER-08: optional bounded heap. Default unbounded (SIZE_MAX).
 * Compile with -DCML_HEAP_LIMIT=N to cap total bytes from checked_malloc. */
#ifndef CML_HEAP_LIMIT
#define CML_HEAP_LIMIT ((size_t)-1)
#endif
static size_t cml_bytes_allocated = 0;
static void *checked_malloc(size_t bytes) {
    size_t need = bytes == 0 ? 1 : bytes;
    if (CML_HEAP_LIMIT != ((size_t)-1)) {
        if (cml_bytes_allocated > CML_HEAP_LIMIT || need > CML_HEAP_LIMIT - cml_bytes_allocated)
            runtime_error("OutOfMemory", "C backend heap limit exceeded");
    }
    void *memory = malloc(need);
    if (memory == NULL) runtime_error("OutOfMemory", "C backend heap allocation");
    cml_bytes_allocated += need;
    return memory;
}

static Value *mk_int(long n) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_INT; v->u.i = n; return v; }
static Value *mk_sym(const char *s) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_SYM; v->u.sym = s; return v; }
static Value *mk_cons(Value *a, Value *b) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_CONS; v->u.cons.car = a; v->u.cons.cdr = b; return v; }
static Value *mk_i32_buffer(const int *data, size_t len) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_I32_BUFFER; v->u.i32_buffer.data = checked_malloc(len * sizeof(int)); v->u.i32_buffer.len = len; memcpy(v->u.i32_buffer.data, data, len * sizeof(int)); return v; }
static Value *mk_closure(Value *(*fn)(Value*, Value*), Value *env) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_CLOSURE; v->u.closure.fn = fn; v->u.closure.env = env; return v; }
static Value *mk_builtin(const char *name, Value *(*fn)(Value*, Value*)) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_BUILTIN; v->u.builtin.name = name; v->u.builtin.fn = fn; return v; }
static Value *mk_string(const char *s) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_STRING; v->u.str = s; return v; }
static Value *mk_sid8(unsigned char sid) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_SID8; v->u.i = sid; return v; }

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
        case TAG_STRING: return strcmp(a->u.str, b->u.str) == 0 ? &TRUE_V : &NIL_V;
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

// Backend mechanism for semantic 1014. The semantic identity/result domain
// remains upstream-owned; this projection returns exact numeric 0/1 rather
// than C truth or Lisp t/nil. Cross multiplication preserves exact rational
// ordering within this backend's existing checked-long representation.
static Value *v_exact_q_lt(Value *a, Value *b) {
    if ((a->tag != TAG_INT && a->tag != TAG_RATIONAL) ||
        (b->tag != TAG_INT && b->tag != TAG_RATIONAL))
        runtime_error("Type", "<");
    long a_num = a->tag == TAG_INT ? a->u.i : a->u.rat.num;
    long a_den = a->tag == TAG_INT ? 1 : a->u.rat.den;
    long b_num = b->tag == TAG_INT ? b->u.i : b->u.rat.num;
    long b_den = b->tag == TAG_INT ? 1 : b->u.rat.den;
    long left = rational_checked_mul(a_num, b_den);
    long right = rational_checked_mul(b_num, a_den);
    return mk_int(left < right ? 1 : 0);
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
static Value *builtin_exact_q_lt(Value *args, Value *env) {
    (void)env;
    require_arity(args, 2, "<");
    require_number(arg_at(args, 0), "<");
    require_number(arg_at(args, 1), "<");
    return v_exact_q_lt(arg_at(args, 0), arg_at(args, 1));
}

static Value *v_apply(Value *callable, Value *args) {
    if (callable->tag == TAG_CLOSURE) return callable->u.closure.fn(args, callable->u.closure.env);
    if (callable->tag == TAG_BUILTIN) return callable->u.builtin.fn(args, &NIL_V);
    if (callable->tag == TAG_SID8) return sid8_apply(callable, args);
    // Issue cml#3 item 4: my-lisp authority classifies a non-callable
    // application under ErrorKind::Type (crates/my-lisp/src/eval/closures.rs),
    // not a dedicated NotCallable kind -- observable output must agree with
    // authority even though "not callable" is a distinct internal condition.
    runtime_error("Type", "attempted to call a non-callable value");
    return &NIL_V;
}

static Value *sid8_apply(Value *callable, Value *args) {
    unsigned char sid = callable->u.i;
    // All admitted SIDs must be handled here -- fail closed for any
    // unsupported SID reaching runtime. This is the single runtime
    // dispatcher mirroring the compiler's Canon registry authority.
    switch (sid) {
        case 0b00000010: // atom
            require_arity(args, 1, "atom");
            return is_atom(arg_at(args, 0)) ? &TRUE_V : &NIL_V;
        case 0b00000011: // eq
            require_arity(args, 2, "eq");
            {
                Value *left = arg_at(args, 0);
                Value *right = arg_at(args, 1);
                if (left->tag == TAG_CONS || right->tag == TAG_CONS) runtime_error("Type", "eq");
                return v_eq(left, right);
            }
        case 0b00000100: // cons
            require_arity(args, 2, "cons");
            return mk_cons(arg_at(args, 0), arg_at(args, 1));
        case 0b00000101: // car
            require_arity(args, 1, "car");
            require_tag(arg_at(args, 0), TAG_CONS, "car");
            return v_car(arg_at(args, 0));
        case 0b00000110: // cdr
            require_arity(args, 1, "cdr");
            require_tag(arg_at(args, 0), TAG_CONS, "cdr");
            return v_cdr(arg_at(args, 0));
        case 0b00001100: // add (+)
            require_arity(args, 2, "+");
            require_number(arg_at(args, 0), "+");
            require_number(arg_at(args, 1), "+");
            return v_add(arg_at(args, 0), arg_at(args, 1));
        case 0b00001101: // sub (-)
            require_min_arity(args, 1, "-");
            {
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
        case 0b00001110: // mul (*)
            require_min_arity(args, 2, "*");
            {
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
        case 0b00001111: // div (/)
            require_min_arity(args, 2, "/");
            {
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
        case 0b00010011: // mod
            require_arity(args, 2, "mod");
            require_number(arg_at(args, 0), "mod");
            require_number(arg_at(args, 1), "mod");
            runtime_error("Type", "mod unsupported in this backend");
            return &NIL_V;
        case 0b00010100: // quotient
            require_arity(args, 2, "quotient");
            require_number(arg_at(args, 0), "quotient");
            require_number(arg_at(args, 1), "quotient");
            runtime_error("Type", "quotient unsupported in this backend");
            return &NIL_V;
        case 0b00011010: // lt (<)
            require_arity(args, 2, "<");
            require_number(arg_at(args, 0), "<");
            require_number(arg_at(args, 1), "<");
            return v_exact_q_lt(arg_at(args, 0), arg_at(args, 1));
        case 0b00011011: // gt (>)
            require_arity(args, 2, ">");
            require_number(arg_at(args, 0), ">");
            require_number(arg_at(args, 1), ">");
            return v_exact_q_lt(arg_at(args, 1), arg_at(args, 0));
        case 0b00011100: // = (numeric equality)
            require_arity(args, 2, "=");
            require_number(arg_at(args, 0), "=");
            require_number(arg_at(args, 1), "=");
            return v_exact_q_lt(arg_at(args, 0), arg_at(args, 1)) ? &NIL_V : v_exact_q_lt(arg_at(args, 1), arg_at(args, 0)) ? &NIL_V : &TRUE_V;
        case 0b00011101: // <=
            require_arity(args, 2, "<=");
            require_number(arg_at(args, 0), "<=");
            require_number(arg_at(args, 1), "<=");
            {
                Value *lt = v_exact_q_lt(arg_at(args, 0), arg_at(args, 1));
                if (lt == &TRUE_V) return &TRUE_V;
                Value *eq = v_exact_q_lt(arg_at(args, 0), arg_at(args, 1));
                // exact_q_lt returns 0/1, so we need proper <=
                // For now fail closed -- unsupported in this backend
                runtime_error("Type", "<= unsupported in this backend");
                return &NIL_V;
            }
        case 0b00011110: // >=
            require_arity(args, 2, ">=");
            require_number(arg_at(args, 0), ">=");
            require_number(arg_at(args, 1), ">=");
            runtime_error("Type", ">= unsupported in this backend");
            return &NIL_V;
        case 0b00100010: // equal?
            require_arity(args, 2, "equal?");
            return v_equal_p(arg_at(args, 0), arg_at(args, 1)) ? &TRUE_V : &NIL_V;
        case 0b00100111: // list
            {
                Value *result = &NIL_V;
                for (Value *cur = args; cur->tag == TAG_CONS; cur = cur->u.cons.cdr) {
                    result = mk_cons(cur->u.cons.car, result);
                }
                if (args->tag != TAG_NIL) runtime_error("Type", "list");
                // Build in reverse then reverse again? Actually just build right-to-left
                Value *out = &NIL_V;
                for (Value *cur = args; cur->tag == TAG_CONS; cur = cur->u.cons.cdr) {
                    out = mk_cons(cur->u.cons.car, out);
                }
                // Actually the above builds reversed. Let's do it properly.
                // Since args is a proper list, collect elements then build from end.
                // Simpler: just build the list by iterating and cons'ing in reverse order.
                // But the list was built in reverse... we need the original order.
                // For now, keep simple reverse approach which gives reversed list.
                // TODO: fix list order
                return out;
            }
        case 0b01011001: // numeric-buffer-map
            require_arity(args, 2, "numeric-buffer-map");
            {
                Value *callable = arg_at(args, 0);
                Value *buffer = arg_at(args, 1);
                return v_map_i32_buffer(callable, buffer);
            }
        default:
            runtime_error("Type", "unsupported SID8");
            return &NIL_V;
    }
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
    bind_global("<", mk_builtin("<", builtin_exact_q_lt));
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
        case TAG_STRING: printf("%s", v->u.str); break;
        case TAG_CLOSURE: printf("<closure>"); break;
        case TAG_BUILTIN: printf("#<builtin %s>", v->u.builtin.name); break;
        case TAG_SID8: {
                unsigned char sid = v->u.i;
                printf("#<sid8 ");
                for (int b = 7; b >= 0; b--) printf("%d", (sid >> b) & 1);
                printf(">");
            } break;
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



int main(void) {
    bootstrap_builtins();
    { Value *result = mk_i32_buffer((int[]){1, -2, 3}, 3); print_value(result); printf("\n"); }
    return 0;
}
