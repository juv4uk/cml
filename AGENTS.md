# AGENTS.md — ecosystem overview for agents working in this repo

This repo (`cml`) is one of four in a coordinated ecosystem. If you're an
agent (Codex, Claude Code, or otherwise) picking up work here, read this
first — it saves you from re-deriving context another agent already has.

## The four repositories

- **my-lisp** — the semantic source of truth. Defines the language: parser,
  evaluator, exactness model (rationals, no floats), `lib/core.my` standard
  library. Language contract version 1.0 (`language-contract.my`). Nothing
  else in the ecosystem may drift from what that repo says the language
  means.
- **fpga-lisp** — hardware implementation of the same language on an FPGA.
  Tracks an ISA contract (`isa-contract.my`, version 0.2) against my-lisp's
  semantics. Currently working through `letrec`-in-closures (plan item 24,
  M28 proved the mechanism, M29 targets the canonical tail-recursive
  `length`/`length-onto` pair) before it can bootstrap
  `reverse`/`append`/`map` from `core.my` — the one open blocker the whole
  ecosystem is waiting on.
- **cml** (this repo) — an Ahead-of-Time compiler from my-lisp source
  directly to fpga-lisp assembly (no runtime `eval`/`apply` loop on the
  hardware). Tracks conformance against both other repos via
  [`compatibility.my`](compatibility.my). Has CI
  (`.github/workflows/ci.yml`) checking out both sibling repos fresh and
  running a real `iverilog` E2E simulation on every push/PR — see
  [`docs/testing.md`](docs/testing.md) for the full pipeline.
- **my-idea** — an observer/IDE layer, depends on my-lisp via
  cargo-git-dependency/submodule. Building toward a "System Observatory"
  panel.

## Machine-readable status

[`ecosystem-status.md`](ecosystem-status.md) in this repo is an append-only
chronological log of cross-session syncs (decisions, verification results,
open questions) — read it before assuming anything is stale or unverified.
my-lisp's `ecosystem-status.my` is the curated current-snapshot counterpart
(no history, just present state); prefer that one for "what's true right
now," this repo's `.md` for "how did we get here."

[`compatibility.my`](compatibility.my) is the actual contract: compiler
version, tested SHAs of my-lisp/fpga-lisp, supported language surface,
per-feature mechanism notes (e.g. `defmacro`, `equal?`), and known
limitations — a flat alist, read via `(read-file "compatibility.my")` from
my-lisp, never `(load ...)`-ed as executable source.

## Talking to my-lisp live

`my-lisp --tcp[=PORT]` (default 9999) starts a REPL reachable over TCP on
`127.0.0.1` only (no auth — same trust boundary as the stdio REPL). Useful
for one-off semantic checks without shelling out to the my-lisp CLI per
call. The my-lisp session can start it on request; connect to
`127.0.0.1:9999` and send one expression per line.

## Conventions worth knowing before editing

- `defmacro` is a **compile-time-only** source transform
  ([`src/macros.rs`](src/macros.rs)) — it never reaches the FPGA compiler.
  See `compatibility.my`'s `defmacro` entry for the mechanism.
- `equal?` is a **native FPGA subroutine** (`cml_equal` in
  [`src/compiler.rs`](src/compiler.rs)), deliberately worklist-based (no
  `CALL`/`RET` recursion) so it doesn't depend on the still-maturing letrec
  mechanism in fpga-lisp.
- The conformance test (`tests/conformance_test.rs`) is a **blind
  adapter**: one fixed pipeline runs unmodified against every fixture — no
  fixture-specific branches inside the adapter itself. Fixtures live in the
  sibling `my-lisp` repo at `tests/fixtures/conformance.my`, not here.
- Requires, checked out as siblings of this repo: `../my-lisp` and
  `../fpga-lisp`, plus `python3` and `iverilog`/`vvp` on `PATH` to run the
  conformance test locally. CI provides all of this fresh on every run, so
  a missing local toolchain blocks local verification only, not landing a
  change — but install what you need yourself rather than skipping local
  verification by default.
- Rust toolchain on this machine is **shared state** across agent
  sessions: the rustup default has been observed flipping between
  `stable-x86_64-pc-windows-msvc` and `stable-x86_64-pc-windows-gnu`
  depending on which session touched it last. Always pass an explicit
  `+stable-x86_64-pc-windows-gnu` (this repo builds GNU-target) rather than
  relying on the default.

## Cross-session coordination protocol (agreed with my-lisp/fpga-lisp)

1. Durable facts go in `ecosystem-status.md`/`ecosystem-status.my` —
   written after the fact (commit done, CI green), not "plan to do X".
2. Direct messages between sessions are for synchronous asks, not
   restating what's already in a status file.
3. Anchor claims to a commit sha or file:line, not a paraphrase from memory.
4. Don't block on confirmation before continuing your own work unless
   there's a real dependency.
