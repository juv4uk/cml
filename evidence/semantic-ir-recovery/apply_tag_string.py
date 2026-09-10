"""COMPILER-03: add TAG_STRING support to c_backend RUNTIME and compile path."""
from pathlib import Path

p = Path("src/c_backend.rs")
t = p.read_text()
if "TAG_STRING" in t and "mk_string" in t and 'Ir::String(s)' in t:
    print("already")
    raise SystemExit(0)

# 1. Tag enum
old_tag = "typedef enum { TAG_NIL, TAG_INT, TAG_SYM, TAG_CONS, TAG_I32_BUFFER, TAG_CLOSURE, TAG_BUILTIN, TAG_RATIONAL } Tag;"
new_tag = "typedef enum { TAG_NIL, TAG_INT, TAG_SYM, TAG_CONS, TAG_I32_BUFFER, TAG_CLOSURE, TAG_BUILTIN, TAG_RATIONAL, TAG_STRING } Tag;"
if old_tag not in t:
    raise SystemExit("tag enum anchor missing")
t = t.replace(old_tag, new_tag, 1)

# 2. union field — after rat field
if "const char *str;" not in t:
    old_union = "struct { long num; long den; } rat;"
    new_union = "struct { long num; long den; } rat;\n        const char *str;"
    if old_union not in t:
        raise SystemExit("union rat anchor missing")
    t = t.replace(old_union, new_union, 1)

# 3. mk_string after mk_builtin
if "mk_string" not in t:
    old_mk = 'static Value *mk_builtin(const char *name, Value *(*fn)(Value*, Value*)) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_BUILTIN; v->u.builtin.name = name; v->u.builtin.fn = fn; return v; }'
    new_mk = old_mk + '\nstatic Value *mk_string(const char *s) { Value *v = checked_malloc(sizeof(Value)); v->tag = TAG_STRING; v->u.str = s; return v; }'
    if old_mk not in t:
        raise SystemExit("mk_builtin anchor missing")
    t = t.replace(old_mk, new_mk, 1)

# 4. v_eq TAG_STRING
if "case TAG_STRING:" not in t or t.count("case TAG_STRING:") < 1:
    old_eq = "case TAG_SYM: return strcmp(a->u.sym, b->u.sym) == 0 ? &TRUE_V : &NIL_V;\n        default: return a == b ? &TRUE_V : &NIL_V;"
    new_eq = "case TAG_SYM: return strcmp(a->u.sym, b->u.sym) == 0 ? &TRUE_V : &NIL_V;\n        case TAG_STRING: return strcmp(a->u.str, b->u.str) == 0 ? &TRUE_V : &NIL_V;\n        default: return a == b ? &TRUE_V : &NIL_V;"
    if old_eq not in t:
        raise SystemExit("v_eq anchor missing")
    t = t.replace(old_eq, new_eq, 1)

# 5. v_equal_p TAG_STRING
old_eqp = "case TAG_SYM: return strcmp(a->u.sym, b->u.sym) == 0;\n        case TAG_CONS:"
new_eqp = "case TAG_SYM: return strcmp(a->u.sym, b->u.sym) == 0;\n        case TAG_STRING: return strcmp(a->u.str, b->u.str) == 0;\n        case TAG_CONS:"
if "case TAG_STRING: return strcmp(a->u.str" not in t:
    if old_eqp not in t:
        raise SystemExit("v_equal_p anchor missing")
    t = t.replace(old_eqp, new_eqp, 1)

# 6. print_value — find case TAG_SYM and add STRING nearby; also RATIONAL block end
if 'case TAG_STRING:' not in t or 'printf("%s"' not in t:
    # insert before default or after SYM in print_value
    marker = 'case TAG_SYM: printf("%s", v->u.sym); break;'
    # actual print may differ — search
    import re
    m = re.search(r'case TAG_SYM:.*?break;', t)
    if not m:
        raise SystemExit("print TAG_SYM missing")
    insert = m.group(0) + '\n        case TAG_STRING: printf("%s", v->u.str); break;'
    if 'case TAG_STRING: printf' not in t:
        t = t.replace(m.group(0), insert, 1)

# 7. Ir::String compile
old_str = 'Ir::String(_) => Err(CompileError::UnsupportedVariant("String")),'
new_str = '''Ir::String(s) => {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                Ok(format!("mk_string(\"{escaped}\")"))
            },'''
if old_str in t:
    t = t.replace(old_str, new_str, 1)
elif 'Ir::String(s)' not in t:
    raise SystemExit("Ir::String arm missing")

# 8. Quoted::Str if present as Unsupported
if 'Quoted::Str' in t and 'UnsupportedVariant("Quoted::Str")' in t:
    t = t.replace(
        'Quoted::Str(_) => Err(CompileError::UnsupportedVariant("Quoted::Str")),',
        '''Quoted::Str(s) => {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                Ok(format!("mk_string(\"{escaped}\")"))
            },''',
        1,
    )

p.write_text(t)
assert "TAG_STRING" in p.read_text()
assert "mk_string" in p.read_text()
print("patched")
