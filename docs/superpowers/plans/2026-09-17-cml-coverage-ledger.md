# CML Coverage Ledger Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first deterministic, machine-readable coverage ledger that enumerates every semantic identity in CML's pinned upstream my-lisp registry and states only what CML can actually prove about admission and backend projection.

**Architecture:** Extend the existing build-time semantic-registry generator so the generated Canon module exposes the complete upstream semantic-ID denominator and a stable registry-content digest. Add a small `coverage` module that joins those generated IDs with `CANON_OPERATIONS_TABLE`; identities absent from the CML operation table are `not-yet-admitted`, admitted identities preserve their CML status/witness, and backend projections are reported only as `projected` or `declared-unsupported` in this first slice. A tiny `cml-coverage` binary renders a stable Lisp-form report; no handwritten coverage percentage or duplicated semantic answer table is introduced.

**Tech Stack:** Rust, Cargo build script, generated `canon_spellings.rs`, CML's vendored my-lisp semantic registry, Lisp/S-expression text output.

**Spec:** GitHub issue #106 (`CML-COVERAGE-LEDGER-1`), coordinated with #27, #46, #84, #105 and #107.

## Global Constraints

- my-lisp owns semantic identity and meaning; CML owns compiler coverage facts only.
- The denominator must come from the real pinned `external/my-lisp` semantic registry, not a handwritten Rust list.
- No coverage row may contain a CML-authored expected Lisp result.
- `projected` is not synonymous with `executable`; executable status requires a later named execution witness.
- Missing CML operation metadata means `not-yet-admitted`, not `unsupported` and not silent omission.
- Backend projection text equal to `unsupported` is reported as `declared-unsupported`; this first slice does not claim #107 runtime rejection is already proven.
- Output ordering must be deterministic by semantic ID.

---

### Task 1: Generate the exact upstream semantic-ID denominator

**Files:**
- Modify: `build.rs`
- Test: `tests/coverage_ledger_test.rs`

**Interfaces:**
- Produces: `canon::UPSTREAM_SEMANTIC_IDS: &[&str]`
- Produces: `canon::UPSTREAM_SEMANTIC_REGISTRY_FNV1A64: u64`
- Consumes: the already-parsed top-level `(sr/1 ...)` registry in `build.rs`

- [ ] **Step 1: Write the failing denominator test**

Create `tests/coverage_ledger_test.rs` with a test that imports `cml::canon::{CANON_OPERATIONS_TABLE, UPSTREAM_SEMANTIC_IDS}` and asserts:

```rust
#[test]
fn upstream_denominator_contains_every_admitted_operation_and_more() {
    let ids: std::collections::BTreeSet<_> = UPSTREAM_SEMANTIC_IDS.iter().copied().collect();
    assert_eq!(ids.len(), UPSTREAM_SEMANTIC_IDS.len(), "upstream semantic IDs must be unique");
    assert!(UPSTREAM_SEMANTIC_IDS.windows(2).all(|w| w[0] < w[1]), "IDs must be sorted");
    for op in CANON_OPERATIONS_TABLE {
        assert!(ids.contains(op.semantic_id), "admitted operation {} missing from upstream denominator", op.semantic_id);
    }
    assert!(UPSTREAM_SEMANTIC_IDS.len() > CANON_OPERATIONS_TABLE.len(), "ledger denominator must include not-yet-admitted upstream identities");
}
```

Also reference `UPSTREAM_SEMANTIC_REGISTRY_FNV1A64` and assert it is non-zero so the generated provenance surface is exercised.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test --test coverage_ledger_test upstream_denominator_contains_every_admitted_operation_and_more -- --exact
```

Expected: compilation fails because `UPSTREAM_SEMANTIC_IDS` / `UPSTREAM_SEMANTIC_REGISTRY_FNV1A64` do not yet exist.

- [ ] **Step 3: Generate the denominator from `root`**

In `build.rs`, after locating the `(sr/1 ...)` root:

1. Collect the first atom of every registry entry whose first atom is a four-digit decimal semantic ID.
2. Reject duplicates and any active occurrence of `RETIRED_SEMANTIC_IDS`.
3. Sort IDs lexicographically (equivalent to numeric order for zero-padded IDs).
4. Emit `pub const UPSTREAM_SEMANTIC_IDS: &[&str] = &[...]` into `canon_spellings.rs`.
5. Compute a deterministic FNV-1a 64-bit digest over the exact registry source bytes and emit `UPSTREAM_SEMANTIC_REGISTRY_FNV1A64`.

Use a local build-script helper:

```rust
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
```

Do not add a dependency solely for hashing.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run the same focused command. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add build.rs tests/coverage_ledger_test.rs
git commit -m "feat(#106): generate upstream semantic coverage denominator"
```

---

### Task 2: Join upstream identities with current CML admission/projection evidence

**Files:**
- Create: `src/coverage.rs`
- Modify: `src/lib.rs`
- Modify: `tests/coverage_ledger_test.rs`

**Interfaces:**
- Produces: `coverage::CoverageState`
- Produces: `coverage::BackendCoverage`
- Produces: `coverage::SemanticCoverageRow`
- Produces: `coverage::supported_pin_rows() -> Vec<SemanticCoverageRow>`

- [ ] **Step 1: Write failing row-classification tests**

Add tests that require:

```rust
let rows = cml::coverage::supported_pin_rows();
assert_eq!(rows.len(), cml::canon::UPSTREAM_SEMANTIC_IDS.len());
assert!(rows.windows(2).all(|w| w[0].semantic_id < w[1].semantic_id));

let mul = rows.iter().find(|r| r.semantic_id == "1002").unwrap();
assert_eq!(mul.admission, cml::coverage::CoverageState::Admitted);
assert!(mul.backends.iter().all(|b| b.state == cml::coverage::CoverageState::DeclaredUnsupported));

let not_admitted = rows.iter().find(|r| r.admission == cml::coverage::CoverageState::NotYetAdmitted).unwrap();
assert!(not_admitted.operation_status.is_none());
assert!(not_admitted.witness.is_none());
```

Also assert every admitted row has a non-empty witness reference.

- [ ] **Step 2: Run the focused test and verify RED**

```bash
cargo test --test coverage_ledger_test -- --nocapture
```

Expected: compilation fails because `cml::coverage` does not exist.

- [ ] **Step 3: Implement the minimal coverage join**

Create `src/coverage.rs` with these public shapes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageState {
    NotYetAdmitted,
    Admitted,
    Projected,
    DeclaredUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCoverage {
    pub backend: &'static str,
    pub state: CoverageState,
    pub projection: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCoverageRow {
    pub semantic_id: &'static str,
    pub admission: CoverageState,
    pub operation_status: Option<&'static str>,
    pub witness: Option<&'static str>,
    pub backends: Vec<BackendCoverage>,
}
```

`pub fn supported_pin_rows()` iterates `UPSTREAM_SEMANTIC_IDS`. For each ID:

- if `canon::find_operation_by_id(id)` is `None`, emit `NotYetAdmitted` with no witness/backend claims;
- otherwise emit `Admitted`, copy `status` and `provenance_witness`, and map every existing `backend_projections` entry to `Projected` unless the projection text is exactly `unsupported`, in which case use `DeclaredUnsupported`.

Do not infer backend evidence from mnemonic names beyond this classification.

Expose the module from `src/lib.rs` with `pub mod coverage;`.

- [ ] **Step 4: Run focused and nearby tests**

```bash
cargo test --test coverage_ledger_test
cargo test --test capability_matrix_test
cargo test --test registry_driven_canon_callables_test
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/coverage.rs src/lib.rs tests/coverage_ledger_test.rs
git commit -m "feat(#106): derive semantic coverage rows from upstream authority"
```

---

### Task 3: Add one stable machine-readable coverage command

**Files:**
- Create: `src/bin/cml-coverage.rs`
- Modify: `tests/coverage_ledger_test.rs`

**Interfaces:**
- Produces command: `cargo run --quiet --bin cml-coverage`
- Output: deterministic Lisp/S-expression text beginning with `(cml-coverage-ledger ...`.

- [ ] **Step 1: Write a failing black-box CLI test**

Add a test using `env!("CARGO_BIN_EXE_cml-coverage")` that runs the binary and requires output to contain:

```text
(kind . cml-coverage-ledger)
(channel . supported-pin)
(registry-fnv1a64 . "...")
(semantic-identities . <N>)
```

and rows for `1002` plus at least one `not-yet-admitted` identity. Do not assert a handwritten total N; compare parsed/output count to `UPSTREAM_SEMANTIC_IDS.len()`.

- [ ] **Step 2: Verify RED**

```bash
cargo test --test coverage_ledger_test coverage_cli_reports_revision_qualified_counts -- --exact
```

Expected: build/test fails because the binary does not exist.

- [ ] **Step 3: Implement deterministic rendering**

Create `src/bin/cml-coverage.rs` that:

1. calls `supported_pin_rows()`;
2. prints `channel = supported-pin`;
3. prints the registry FNV-1a digest in fixed 16-digit lowercase hex;
4. prints total/upstream, admitted and not-yet-admitted counts;
5. prints every row sorted by semantic ID, including operation status/witness only when present and backend projection state only when present.

Do not print a percentage in this first slice.

- [ ] **Step 4: Verify CLI + full coverage tests**

```bash
cargo test --test coverage_ledger_test
cargo run --quiet --bin cml-coverage > /tmp/cml-coverage.lisp
head -40 /tmp/cml-coverage.lisp
```

Expected: tests PASS; report is deterministic and includes both admitted and not-yet-admitted rows.

- [ ] **Step 5: Commit**

```bash
git add src/bin/cml-coverage.rs tests/coverage_ledger_test.rs
git commit -m "feat(#106): expose machine-readable CML coverage ledger"
```

---

### Task 4: Prove ledger honesty invariants and run full verification

**Files:**
- Modify: `tests/coverage_ledger_test.rs`

**Interfaces:**
- Consumes all preceding coverage APIs and CLI.
- Produces CI-level drift guards for the first ledger slice.

- [ ] **Step 1: Add invariant tests**

Require:

- every upstream ID occurs exactly once;
- every CML operation ID exists upstream;
- every admitted row has a non-empty provenance witness;
- a `Projected` backend state always has non-`unsupported` projection text;
- a `DeclaredUnsupported` backend state always has projection text exactly `unsupported`;
- `NotYetAdmitted` rows carry no backend/evidence claims;
- `1002` is admitted but backend-declared-unsupported after merged #103, proving admission != execution.

- [ ] **Step 2: Run focused verification**

```bash
cargo fmt -- --check
cargo test --test coverage_ledger_test
cargo test --test capability_matrix_test
cargo test --test canon_operations_table_test
```

Expected: PASS.

- [ ] **Step 3: Run the full suite**

```bash
cargo test --verbose
```

Expected: PASS with no existing backend/conformance regressions.

- [ ] **Step 4: Commit final invariant changes**

```bash
git add tests/coverage_ledger_test.rs
git commit -m "test(#106): guard coverage ledger evidence boundaries"
```

- [ ] **Step 5: Record issue evidence**

Update #106 with exact branch/head, focused/full CI run links, upstream registry digest, total upstream identity count and admitted/not-yet-admitted counts. Explicitly state that `projected` is not yet an `executable` claim.
