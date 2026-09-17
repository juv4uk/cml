# Exact Rational Authority Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove CML's already-existing C-backend exact-rational execution against the pinned my-lisp Lisp-owned conformance corpus, then retire duplicated hard-coded rational expected values from Rust tests.

**Architecture:** Keep production compiler/runtime code unchanged in this slice. Reuse the existing `external/my-lisp` pin and C backend pipeline (`parse -> lower_program_with_first_class_builtins -> CBackend -> gcc -> process output`). Select an upstream exact-rational `compiler-corpus` row, extract both source and expected value from that upstream row, and compare CML's actual output against it. This converts a local Rust semantic claim into a cross-repo evidence path without touching `src/ir.rs` or `src/lower.rs`, which are active write lanes for #91/#108.

**Tech Stack:** Rust integration tests, CML parser/lowering/C backend, GCC, pinned `external/my-lisp` submodule.

**Spec:** GitHub issue `juv4uk/cml#105` plus witness-authority contract `juv4uk/cml#46`.

## Global Constraints

- my-lisp owns expected exact-rational semantics; CML test code must not duplicate the expected rational answer.
- Use the existing pinned submodule revision; do not bump #84 revision channels in this slice.
- Do not modify `src/ir.rs`, `src/lower.rs`, #91/#108 production files, or backend semantics.
- Keep the existing C backend exact-rational mechanism unchanged unless the upstream-driven witness exposes a real defect.
- Preserve GCC execution as mechanism only; GCC is not the semantic oracle.

---

### Task 1: RED — require upstream-owned exact-rational C-backend witness

**Files:**
- Modify: `tests/c_backend_test.rs`

**Interfaces:**
- Consumes: existing `compile_and_run_first_class(code, stem) -> String` helper.
- Produces: a test call to `upstream_exact_rational_compiler_witness()` returning `(String, String)` source + expected answer sourced from upstream corpus.

- [ ] **Step 1: Replace one local exact-rational expected-value assertion with a call to a not-yet-defined upstream witness selector**

Add a focused test near the existing C4 exact-rational block:

```rust
#[test]
fn c_backend_exact_rational_result_is_owned_by_upstream_lisp_witness() {
    let (source, expected) = upstream_exact_rational_compiler_witness();
    assert_eq!(
        compile_and_run_first_class(&source, "upstream_exact_rational"),
        expected
    );
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test --test c_backend_test c_backend_exact_rational_result_is_owned_by_upstream_lisp_witness -- --nocapture
```

Expected: compilation fails because `upstream_exact_rational_compiler_witness` is not defined. This proves the new authority path is missing rather than silently testing an existing local expectation.

- [ ] **Step 3: Commit RED**

```bash
git add tests/c_backend_test.rs
git commit -m "test(#105): demand upstream-owned exact-rational C witness"
```

---

### Task 2: GREEN — consume the pinned upstream compiler-corpus rational row

**Files:**
- Modify: `tests/c_backend_test.rs`

**Interfaces:**
- Consumes: `external/my-lisp/tests/fixtures/conformance.lisp` from the pinned submodule.
- Produces: `upstream_exact_rational_compiler_witness() -> (String, String)`.

- [ ] **Step 1: Add a minimal upstream-corpus reader and selector**

Add imports:

```rust
use std::path::PathBuf;
```

Add helpers:

```rust
fn upstream_conformance_corpus() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp/tests/fixtures/conformance.lisp");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("missing pinned upstream conformance corpus at {}: {error}", path.display())
    })
}

fn alist_string_field(line: &str, key: &str) -> Option<String> {
    let marker = format!("({key} . \\\"");
    let tail = line.split_once(&marker)?.1;
    Some(tail.split_once("\\\")")?.0.to_string())
}

fn upstream_exact_rational_compiler_witness() -> (String, String) {
    upstream_conformance_corpus()
        .lines()
        .filter(|line| !line.trim_start().starts_with(';'))
        .find_map(|line| {
            if !line.contains("(compiler-corpus . t)") {
                return None;
            }
            let source = alist_string_field(line, "expr")?;
            let expected = alist_string_field(line, "expected")?;
            if expected.contains('/') {
                Some((source, expected))
            } else {
                None
            }
        })
        .expect("#105 requires at least one exact-rational compiler-corpus row in pinned my-lisp")
}
```

- [ ] **Step 2: Run the focused test and verify GREEN**

Run:

```bash
cargo test --test c_backend_test c_backend_exact_rational_result_is_owned_by_upstream_lisp_witness -- --nocapture
```

Expected: PASS. The actual result comes from the compiled C artifact; the expected exact rational comes only from the pinned upstream Lisp-authored row.

- [ ] **Step 3: Run the complete C backend integration suite**

Run:

```bash
cargo test --test c_backend_test -- --nocapture
```

Expected: PASS.

- [ ] **Step 4: Commit GREEN**

```bash
git add tests/c_backend_test.rs
git commit -m "test(#105): consume upstream exact-rational witness"
```

---

### Task 3: Authority cleanup — retire duplicated Rust exact-rational answers

**Files:**
- Modify: `tests/c_backend_test.rs`

**Interfaces:**
- Consumes: GREEN upstream-driven witness from Task 2.
- Produces: no CML-local exact-rational answer table for the migrated proof.

- [ ] **Step 1: Remove the four hard-coded C4 exact-rational expected-value tests**

Delete the local assertions whose expected values are embedded in Rust for rational add/sub/mul/div. Keep mechanism/error tests that do not duplicate semantic truth.

- [ ] **Step 2: Run focused + full C backend tests**

Run:

```bash
cargo test --test c_backend_test c_backend_exact_rational_result_is_owned_by_upstream_lisp_witness -- --nocapture
cargo test --test c_backend_test -- --nocapture
```

Expected: PASS.

- [ ] **Step 3: Run full CML suite**

Run:

```bash
cargo test --verbose
```

Expected: PASS.

- [ ] **Step 4: Commit authority cleanup**

```bash
git add tests/c_backend_test.rs
git commit -m "cleanup(#105): retire local exact-rational answer assertions"
```

---

### Task 4: Coordination and evidence

**Files:**
- No production files.
- Update GitHub issue `#105` and PR body.

**Interfaces:**
- Consumes: exact-head CI evidence.
- Produces: a clear split between already-proven C-backend exact-Q support and still-missing x86/general arbitrary-precision coverage.

- [ ] **Step 1: Record exact evidence**

Document pinned upstream SHA, selected upstream row, CML head SHA, focused C-backend test result, and full-suite result.

- [ ] **Step 2: Update #105 progress**

Mark the first non-integer exact-rational C-backend vertical as evidence-backed, but explicitly leave open:

```text
x86-freestanding: representation-limited (Ir::Rational rejected)
arbitrary precision beyond host long: not proven
GPU/FPGA: explicitly unsupported/not yet admitted as applicable
```

- [ ] **Step 3: Coordinate #106**

Tell PR #117 that the coverage ledger must represent C backend exact-rational execution separately from x86 representation-limited status; do not collapse them into one `exact-rational-supported` Boolean.

## Self-review

- Spec coverage: converts current C-backend exact rational capability into upstream-owned evidence and preserves #46 authority.
- Placeholder scan: no TODO/TBD placeholders.
- Type consistency: helper returns `(String, String)` and the test passes `&source` plus owned `expected` to existing String-returning helper.
- Collision check: no writes to `src/ir.rs`, `src/lower.rs`, #91/#108 paths, or #117 coverage implementation files.
