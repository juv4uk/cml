"""Update c_backend_test non-callable assertion for COMPILER-05 NotCallable."""
from pathlib import Path

p = Path("tests/c_backend_test.rs")
t = p.read_text()
old = 'assert!(String::from_utf8_lossy(&run.stderr).starts_with("Type:"));'
new = '''let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.starts_with("NotCallable:") || stderr.starts_with("Type:"),
        "expected NotCallable: or Type:, got {stderr:?}"
    );'''
if "NotCallable:" in t and "non_callable" in t and old not in t:
    print("already")
    raise SystemExit(0)
if old not in t:
    raise SystemExit("anchor missing")
p.write_text(t.replace(old, new, 1))
print("patched")
