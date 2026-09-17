# CML Coverage Ledger Foundation Design

## Purpose

CML needs a machine-readable answer to a narrow first question before it can make honest completeness claims:

> Which semantic identities exist in the exact upstream registry pinned by this checkout, and which of those identities are currently admitted into CML's compiler operation table?

This foundation deliberately does **not** claim backend executability yet. It establishes the denominator and the first compiler-state dimension so later work can add backend execution, explicit rejection, representation limits, and physical evidence without inventing a second semantic authority.

## Authority boundary

- `external/my-lisp/lib/surface/semantic-registry.lisp` is the upstream identity source consumed by the existing CML build.
- `build.rs` already parses that registry and generates Canon tables. The ledger reuses that same parse, so there is no second registry parser in production code.
- `CANON_OPERATIONS_TABLE` remains the current CML admission projection for known operations.
- CML records whether an identity is admitted; CML does not copy or redefine what that identity means.
- Semantic expected values remain upstream Lisp-owned evidence under #46.

## Provenance naming

The repository currently has unresolved revision-channel drift tracked by #84: `compatibility.lisp`'s historical `tested-sha` does not equal the actual `external/my-lisp` gitlink used by the build.

Therefore this slice names its source channel **`pinned-submodule`**. It must not call that pin `supported-pin` or `observed-current` until #84 ratifies those roles.

For deterministic identity of the exact registry content, `build.rs` will emit a stable FNV-1a 64-bit digest of the registry bytes. This digest is content provenance, not semantic authority and not a replacement for the future #84 revision channel.

## Generated denominator

`build.rs` will generate two additional constants into `canon_spellings.rs`:

```rust
pub const CANON_UPSTREAM_SEMANTIC_IDS: &[&str];
pub const CANON_UPSTREAM_REGISTRY_FNV1A64: u64;
```

The ID list is collected from every direct row of the authoritative `(sr/1 ...)` form.

Build-time validation must fail closed if:

- a row has no first atom;
- an ID contains a non-ASCII digit;
- an ID is duplicated;
- an admitted `CANON_OPERATIONS_TABLE` semantic ID is absent from the upstream denominator.

Opaque IDs are preserved lexically (`"0001"`, not integer `1`).

## Coverage model

New module: `src/coverage.rs`.

First-slice state model:

```rust
pub enum AdmissionState {
    SourceAdmitted,
    NotYetAdmitted,
}

pub struct SemanticCoverageRow {
    pub semantic_id: &'static str,
    pub admission: AdmissionState,
    pub operation_status: Option<&'static str>,
    pub evidence: Option<&'static str>,
}

pub struct CoverageLedger {
    pub upstream_channel: &'static str,
    pub registry_digest_fnv1a64: u64,
    pub rows: Vec<SemanticCoverageRow>,
}
```

`CoverageLedger::pinned_submodule()` iterates every generated upstream ID exactly once. `canon::find_operation_by_id` determines whether that identity is currently admitted.

For an admitted identity:

- `admission = SourceAdmitted`;
- `operation_status` comes directly from the generated Canon operation row;
- `evidence` comes directly from `provenance_witness`.

For an unadmitted identity:

- `admission = NotYetAdmitted`;
- `operation_status = None`;
- `evidence = None`.

No backend state is inferred from an operation's mere existence.

## Command surface

Add a small binary `src/bin/cml-coverage.rs`.

Default invocation:

```text
cargo run --bin cml-coverage
```

It prints deterministic Lisp-shaped data, for example:

```lisp
(cml-coverage/1
  (upstream-channel pinned-submodule)
  (registry-fnv1a64 0123456789abcdef)
  (semantic-identities 161)
  (source-admitted 20)
  (not-yet-admitted 141)
  (rows
    ((0001 source-admitted supported "...")
     (....))))
```

Counts are derived from rows. No handwritten percentage or expected Lisp result appears in the output.

## Testing strategy

TDD first.

A new `tests/coverage_ledger_test.rs` will require:

1. every generated upstream ID is numeric-only and unique;
2. every admitted `CANON_OPERATIONS_TABLE` identity appears in the denominator;
3. `CoverageLedger::pinned_submodule()` has exactly one row per upstream ID;
4. admitted rows carry operation status and evidence;
5. at least one upstream identity is visibly `NotYetAdmitted`, proving the ledger does not equate observation with support;
6. summary counts exactly partition the denominator;
7. formatted CLI projection is deterministic and contains no percentage claim.

The RED is expected because the generated denominator and `coverage` module do not exist on current master.

## Coordination

This slice does not modify:

- `src/ir.rs` / `src/lower.rs` owned by active #108/#91 work;
- `tests/exact_q_*` owned by #108/#116/#95 work;
- canonical CondMatch implementation owned by #91/#98;
- backend capability claims in `capability-matrix.lisp`.

The next #106 slice will reconcile per-semantic backend states with `capability-matrix.lisp` and #107 explicit rejection evidence.

## Non-goals

- No claim that an admitted operation executes on every backend.
- No `supported-pin` label before #84 resolves revision roles.
- No percentage of language completion in this first slice.
- No changes to Lisp semantics, exact-Q lowering, optimizer policy, or backend codegen.
- No new third-party dependency.

## Success criterion

After this slice, CML can produce a deterministic evidence-bearing denominator and answer, for every pinned upstream semantic identity, whether CML currently admits it at the compiler-operation level. That is the first honest layer of #106, not the final completeness ledger.