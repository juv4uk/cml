from pathlib import Path

p = Path("src/c_backend.rs")
t = p.read_text()
if 'runtime_error("NotCallable", "attempted to call a non-callable value")' in t:
    print("already")
    raise SystemExit(0)
old = 'runtime_error("Type", "attempted to call a non-callable value");'
new = 'runtime_error("NotCallable", "attempted to call a non-callable value");'
if old not in t:
    raise SystemExit("anchor missing")
p.write_text(t.replace(old, new, 1))
assert 'runtime_error("NotCallable", "attempted to call a non-callable value")' in p.read_text()
print("patched")
