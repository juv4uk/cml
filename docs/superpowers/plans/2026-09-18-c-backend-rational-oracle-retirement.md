# C-backend exact-rational oracle retirement plan

**Issue:** #123  
**Base:** `master@2762b9001e3a43cb06e84ec586758cf91db57b4f`  
**Authority:** #46 — semantic expected values belong to pinned my-lisp Lisp-authored conformance data.

## Goal

Retire the exact-rational answer table embedded in `tests/c_backend_test.rs` without changing C-backend production semantics. Preserve every semantic class through executable upstream-owned witnesses first.

## Non-goals

- no changes to `src/c_backend.rs`, `src/ir.rs`, or `src/lower.rs`;
- no x86-freestanding rational representation work;
- no arbitrary-precision claim beyond evidence actually executed;
- no edits to #106 coverage implementation files.

## Task 1 — RED authority-hygiene gate

Create an authority-hygiene integration test that reads `tests/c_backend_test.rs` and rejects the complete known stale local rational semantic-answer block:

- `c_backend_lowers_rational_add_matches_oracle`
- `c_backend_lowers_rational_sub_matches_oracle`
- `c_backend_lowers_rational_mul_matches_oracle`
- `c_backend_lowers_rational_div_matches_oracle`
- `c_backend_lowers_rational_reduces_matches_oracle`
- `c_backend_lowers_rational_int_mix_matches_oracle`
- `c_backend_lowers_rational_unary_minus_matches_oracle`

Run the focused test and record RED because those local Rust oracles still exist.

For the staged preservation run, keep the same gate but name it `tests/zz_exact_rational_authority_hygiene_test.rs` so Cargo executes the upstream preservation witness before the intentionally failing hygiene gate. This ordering change does not weaken the assertion; it makes the preservation-before-subtraction evidence observable in one CI run.

## Task 2 — preservation-first upstream witness matrix

Extend `tests/exact_rational_upstream_authority_test.rs` so it selects rows from pinned `external/my-lisp/tests/fixtures/conformance.lisp`, takes both source and expected from that file, compiles the source through the real C backend, GCC and native execution, and compares runtime output only with the upstream expected field.

Required semantic classes and pinned-source selectors:

- add/reduction: `(+ (/ 1 3) (/ 1 3))`
- subtraction + int/rational mixing: `(- 1 (/ 1 3))`
- multiplication/reduction: `(* (/ 2 3) (/ 9 4))`
- division with rational intermediate: `(/ 5 6 8 7)`
- unary minus: `(- (/ 1 3))`

The selectors identify required language cases; **no expected answer may be copied into Rust**.

Run this preservation suite before deleting the old local answer block. All cases must be GREEN.

## Task 3 — subtract the second answer key

Delete the seven stale rational `*_matches_oracle` tests from `tests/c_backend_test.rs` as one bounded block. Preserve mechanism/error tests outside that block.

Run:

- `cargo test --test exact_rational_upstream_authority_test`
- `cargo test --test zz_exact_rational_authority_hygiene_test`
- `cargo test --test c_backend_test`

Expected: all GREEN.

## Task 4 — repair evidence pointers, not semantics

If `capability-matrix.lisp` still names the deleted rational tests as evidence, retarget only the rational evidence string to `exact_rational_upstream_authority_test.rs` and describe it as pinned upstream-owned execution evidence. Do not change capability status or semantic values.

Run relevant matrix tests.

## Task 5 — full verification and coordination

Run full CI on exact head. Before merge:

1. verify `master` has not moved incompatibly;
2. compare branch vs current base and confirm no production files changed;
3. post exact-head evidence to #123 and #117/#106;
4. mark ready and merge only after exact-head GREEN.

## Success condition

CML still executes the same exact-rational semantics through the C backend, but the semantic answers are sourced from pinned Lisp authority rather than duplicated in the C-backend Rust test table.
