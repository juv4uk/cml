"""COMPILER-08: add optional CML_HEAP_LIMIT accounting to c_backend RUNTIME."""
from pathlib import Path

p = Path("src/c_backend.rs")
t = p.read_text()
if "CML_HEAP_LIMIT" in t and "cml_bytes_allocated" in t:
    print("already")
    raise SystemExit(0)

old = """static void runtime_error(const char *kind, const char *detail);
static void *checked_malloc(size_t bytes) {
    void *memory = malloc(bytes == 0 ? 1 : bytes);
    if (memory == NULL) runtime_error("OutOfMemory", "C backend heap allocation");
    return memory;
}"""

new = """static void runtime_error(const char *kind, const char *detail);
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
}"""

if old not in t:
    raise SystemExit("checked_malloc anchor missing")
p.write_text(t.replace(old, new, 1))
assert "CML_HEAP_LIMIT" in p.read_text()
print("patched")
