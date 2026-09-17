# CML Coverage Ledger Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate an exact upstream semantic-ID denominator from the pinned my-lisp registry and expose a deterministic source-admission coverage ledger without claiming backend executability.

**Architecture:** Reuse `build.rs`'s existing authoritative semantic-registry parse to generate opaque upstream IDs plus a deterministic content digest. A new `coverage` module joins those generated IDs against `CANON_OPERATIONS_TABLE`; a tiny CLI renders the result as Lisp-shaped evidence data. Backend support remains explicitly out of scope for this slice.

**Tech Stack:** Rust 2024, existing CML build-script S-expression parser, generated `canon_spellings.rs`, no new crates.

**Spec:** `docs/superpowers/specs/2026-09-18-cml-coverage-ledger-foundation-design.md`

## Global Constraints

- Upstream language identity remains owned by `external/my-lisp/lib/surface/semantic-registry.lisp`.
- Do not touch `src/ir.rs`, `src/lower.rs`, active `tests/exact_q_*`, or CondMatch production files owned by #108/#116/#91/#98.
- Do not call the current gitlink `supported-pin`; first slice uses `pinned-submodule` until #84 resolves channel roles.
- No backend executability claim in this slice.
- No semantic expected values copied into CML.
- No new dependency.

---

### Task 1: RED coverage API contract

**Files:**
- Create: `tests/coverage_ledger_test.rs`

**Interfaces:**
- Consumes: planned `cml::coverage::{AdmissionState, CoverageLedger}` and generated `cml::canon::CANON_UPSTREAM_SEMANTIC_IDS`.
- Produces: executable requirements for denominator uniqueness, admission partition, provenance and summary counts.

- [ ] **Step 1: Write the failing integration test**

Create tests that require:

```rust
use std::collections::BTreeSet;
use cml::canon::{CANON_OPERATIONS_TABLE, CANON_UPSTREAM_SEMANTIC_IDS};
use cml::coverage::{AdmissionState, CoverageLedger};

#[test]
fn pinned_submodule_ledger_covers_every_upstream_identity_once() {
    let ledger = CoverageLedger::pinned_submodule();
    assert_eq!(ledger.upstream_channel, "pinned-submodule");
    assert_eq!(ledger.rows.len(), CANON_UPSTREAM_SEMANTIC_IDS.len());

    let ids: BTreeSet<_> = ledger.rows.iter().map(|row| row.semantic_id).collect();
    assert_eq!(ids.len(), ledger.rows.len());
    assert!(ids.iter().all(|id| id.chars().all(|c| c.is_ascii_digit())));
}

#[test]
fn every_admitted_operation_is_visible_with_evidence() {
    let ledger = CoverageLedger::pinned_submodule();
    for op in CANON_OPERATIONS_TABLE {
        let row = ledger.row(op.semantic_id).expect("admitted ID must be in denominator");
        assert_eq!(row.admission, AdmissionState::SourceAdmitted);
        assert_eq!(row.operation_status, Some(op.status));
        assert_eq!(row.evidence, Some(op.provenance_witness));
    }
}

#[test]
fn unadmitted_upstream_identity_is_not_misreported_as_supported() {
    let ledger = CoverageLedger::pinned_submodule();
    assert!(ledger.rows.iter().any(|row| row.admission == AdmissionState::NotYetAdmitted));
}

#[test]
fn summary_partitions_the_denominator() {
    let ledger = CoverageLedger::pinned_submodule();
    let summary = ledger.summary();
    assert_eq!(summary.semantic_identities, ledger.rows.len());
    assert_eq!(summary.source_admitted + summary.not_yet_admitted, summary.semantic_identities);
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test --test coverage_ledger_test --verbose
```

Expected: compile failure because `cml::coverage` and `CANON_UPSTREAM_SEMANTIC_IDS` do not exist yet. Record this exact RED in the draft PR before adding production code.

- [ ] **Step 3: Commit RED only**

```bash
git add tests/coverage_ledger_test.rs
git commit -m "test(#106): demand upstream semantic coverage denominator"
```

---

### Task 2: Generate authoritative upstream semantic IDs and registry digest

**Files:**
- Modify: `build.rs`

**Interfaces:**
- Produces: `CANON_UPSTREAM_SEMANTIC_IDS: &[&str]`, `CANON_UPSTREAM_REGISTRY_FNV1A64: u64` in generated `canon_spellings.rs`.
- Consumes: existing `root` slice from the parsed `(sr/1 ...)` registry.

- [ ] **Step 1: Add a build-time helper that validates and collects row IDs**

Implement a helper shaped like:

```rust
fn collect_semantic_ids(root: &[Sexp]) -> Vec<String> {
    let mut ids = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for row in root {
        let Sexp::List(items) = row else {
            panic!("cml#106: semantic-registry row must be a list");
        };
        let Some(Sexp::Atom(id)) = items.first() else {
            panic!("cml#106: semantic-registry row must begin with opaque semantic ID");
        };
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            panic!("cml#106: semantic ID must contain ASCII digits only: {id:?}");
        }
        if !seen.insert(id.clone()) {
            panic!("cml#106: duplicate semantic ID {id}");
        }
        ids.push(id.clone());
    }

    ids
}
```

- [ ] **Step 2: Add deterministic FNV-1a 64-bit content digest**

Implement without dependencies:

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

- [ ] **Step 3: Validate every admitted operation is upstream-known**

After collecting IDs, construct a set and require every `OPERATIONS` semantic ID to exist. Missing IDs must panic with `cml#106` context.

- [ ] **Step 4: Emit both generated constants**

Append to generated `canon_spellings.rs`:

```rust
pub const CANON_UPSTREAM_SEMANTIC_IDS: &[&str] = &[
    "0001",
    // ... generated from registry rows
];

pub const CANON_UPSTREAM_REGISTRY_FNV1A64: u64 = 0x...;
```

- [ ] **Step 5: Run focused test**

Run:

```bash
cargo test --test coverage_ledger_test --verbose
```

Expected: RED advances from missing generated denominator to missing `cml::coverage` API.

- [ ] **Step 6: Commit generated-denominator mechanism**

```bash
git add build.rs
git commit -m "feat(#106): generate upstream semantic identity denominator"
```

---

### Task 3: Implement typed admission ledger

**Files:**
- Create: `src/coverage.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `canon::CANON_UPSTREAM_SEMANTIC_IDS`, `canon::CANON_UPSTREAM_REGISTRY_FNV1A64`, `canon::find_operation_by_id`.
- Produces: `AdmissionState`, `SemanticCoverageRow`, `CoverageSummary`, `CoverageLedger`.

- [ ] **Step 1: Expose the module**

Add to `src/lib.rs`:

```rust
pub mod coverage;
```

- [ ] **Step 2: Implement minimal typed rows**

Use:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionState {
    SourceAdmitted,
    NotYetAdmitted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCoverageRow {
    pub semantic_id: &'static str,
    pub admission: AdmissionState,
    pub operation_status: Option<&'static str>,
    pub evidence: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageSummary {
    pub semantic_identities: usize,
    pub source_admitted: usize,
    pub not_yet_admitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageLedger {
    pub upstream_channel: &'static str,
    pub registry_digest_fnv1a64: u64,
    pub rows: Vec<SemanticCoverageRow>,
}
```

- [ ] **Step 3: Implement `CoverageLedger::pinned_submodule()`**

Iterate generated upstream IDs in order. If `find_operation_by_id(id)` returns an operation, emit `SourceAdmitted` with its status/evidence; otherwise emit `NotYetAdmitted` with no status/evidence.

- [ ] **Step 4: Implement lookup and summary**

Add:

```rust
pub fn row(&self, semantic_id: &str) -> Option<&SemanticCoverageRow>;
pub fn summary(&self) -> CoverageSummary;
```

Counts are derived from `rows` only.

- [ ] **Step 5: Run focused test and verify GREEN**

```bash
cargo test --test coverage_ledger_test --verbose
```

Expected: PASS.

- [ ] **Step 6: Run relevant regression tests**

```bash
cargo test --test capability_matrix_test --test canon_dispatch_test --test registry_driven_canon_callables_test --verbose
```

Expected: PASS.

- [ ] **Step 7: Commit typed ledger**

```bash
git add src/lib.rs src/coverage.rs tests/coverage_ledger_test.rs
git commit -m "feat(#106): add typed source-admission coverage ledger"
```

---

### Task 4: Add deterministic command projection

**Files:**
- Create: `src/bin/cml-coverage.rs`
- Modify: `tests/coverage_ledger_test.rs`

**Interfaces:**
- Consumes: `CoverageLedger::pinned_submodule()`.
- Produces: deterministic Lisp-shaped text to stdout.

- [ ] **Step 1: Extend RED test for formatter contract**

Add a test requiring a library formatter `ledger.to_lisp()` (implemented in `src/coverage.rs`) or equivalent pure function. Require:

```rust
let text = CoverageLedger::pinned_submodule().to_lisp();
assert!(text.starts_with("(cml-coverage/1\n"));
assert!(text.contains("(upstream-channel pinned-submodule)"));
assert!(text.contains("(semantic-identities "));
assert!(text.contains("(source-admitted "));
assert!(text.contains("(not-yet-admitted "));
assert!(!text.contains('%'));
```

Run focused test and verify it fails because formatter is missing.

- [ ] **Step 2: Implement deterministic formatter**

Render rows in upstream registry order. Use fixed symbols `source-admitted` and `not-yet-admitted`; admitted evidence strings must be escaped as quoted Lisp strings.

- [ ] **Step 3: Add CLI wrapper**

`src/bin/cml-coverage.rs` should contain only:

```rust
use cml::coverage::CoverageLedger;

fn main() {
    print!("{}", CoverageLedger::pinned_submodule().to_lisp());
}
```

- [ ] **Step 4: Verify focused tests and command**

```bash
cargo test --test coverage_ledger_test --verbose
cargo run --quiet --bin cml-coverage > /tmp/cml-coverage.lisp
head -n 8 /tmp/cml-coverage.lisp
```

Expected: deterministic `cml-coverage/1` output with counts partitioning the denominator.

- [ ] **Step 5: Commit command projection**

```bash
git add src/coverage.rs src/bin/cml-coverage.rs tests/coverage_ledger_test.rs
git commit -m "feat(#106): expose machine-readable coverage ledger command"
```

---

### Task 5: Verification and handoff to backend-state slice

**Files:**
- No production file required unless verification exposes a defect.
- Update PR body / #106 issue commentary with evidence.

**Interfaces:**
- Produces: exact-head proof and next-slice boundary for `capability-matrix.lisp` + #107 integration.

- [ ] **Step 1: Run formatting**

```bash
cargo fmt -p cml -- --check
```

Expected: PASS.

- [ ] **Step 2: Run full suite**

```bash
cargo test --verbose
```

Expected: PASS.

- [ ] **Step 3: Inspect generated ledger**

```bash
cargo run --quiet --bin cml-coverage
```

Confirm counts are derived, every row has one upstream ID, admitted rows have evidence, and no backend executable claim appears.

- [ ] **Step 4: Record exact-head CI**

Require GitHub Actions GREEN on the final commit before merge claims.

- [ ] **Step 5: Handoff next slice**

Update #106 with the next non-overlapping task: map semantic identities to backend multi-state coverage by reconciling `capability-matrix.lisp`, operation backend projections, executable witnesses and #107 named rejection evidence. Do not infer backend support from source admission alone.
