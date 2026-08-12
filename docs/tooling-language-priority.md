# Tooling language priority: which cml modules should move to Lisp

Analysis written 2026-08-12, after `docs/heterogeneous-backends.md`'s
step 1/1.5 landed (`ir.rs`/`lower.rs`, `compiler.rs` consuming `Ir`) and
the question came up of whether `cml` itself — a compiler for a Lisp,
written in Rust — should follow `fpga-lisp`'s `assembler.py` →
`assembler.my` self-hosting move. Uses the same dividing line
`fpga-lisp/docs/tooling-language-priority.md` already established, applied
here to `cml`'s own modules instead of re-deriving it.

## Inventory

| Module | Lines | What it does | Data in / out |
|---|---|---|---|
| `parser.rs` | 108 | `.my` text → `ast::Expr` | text → Lisp data |
| `macros.rs` | 207 | `defmacro` expansion (compile-time only, never reaches fpga-lisp) | Lisp data → Lisp data |
| `lower.rs` | 170 | `ast::Expr` → `ir::Ir` | Lisp data → typed IR |
| `ir.rs` | 96 | IR type definitions | types only, no I/O |
| `compiler.rs` | 516 | `ir::Ir` → fpga-lisp ISA assembly text, register allocation | typed IR → hardware-adjacent bytes |

## The dividing line (restated from fpga-lisp)

**Migrate to Lisp where the tool transforms Lisp data. Don't, where
correctness depends on static types catching a class of bug that a
dynamically-typed reimplementation would have to re-earn by hand.**
fpga-lisp's version of this line was about *hardware I/O* (UART/serial);
`cml`'s version is about *register-allocation correctness* — this
session found three real bugs (`e73f93a`, `166dffa`, `2b66898`) exactly
where an un-typed, hand-maintained invariant (which registers survive a
nested call) was silently violated. That's the `cml`-specific reason to
keep the codegen layer in a statically-checked language, not a
transplanted rule.

## Per-module analysis

**`macros.rs` — highest priority, real self-hosting candidate.**
`defmacro` expansion is a tree-walking meta-evaluator over `quote`/
`cons`/`car`/`cdr`/`atom`/`eq`/`cond` (`compatibility.my`'s own
description) that never reaches fpga-lisp at all (`never-reaches-fpga .
true`). It is, structurally, a small Lisp interpreter written in Rust —
the exact shape `assembler.py`→`assembler.my` already proved worth
porting, and arguably a stronger case: a macro-expander written *in* the
language whose macros it expands is the self-hosting move, not just a
tool that happens to consume Lisp-shaped data.

**`lower.rs` — Lisp-shaped, but coupled to `ir.rs`'s Rust types.**
`ast::Expr -> Ir` is a pure data transformation and reads as naturally
in Lisp as `macros.rs` does. Lower priority than `macros.rs` in
practice: its output (`Ir`) is consumed directly by `compiler.rs` as
native Rust types, so porting `lower.rs` alone would need a
serialization boundary back into Rust that doesn't exist today — a real
project, not a small one, and not obviously worth it unless `ir.rs`
moves too (see below, it shouldn't).

**`parser.rs` — Lisp-shaped in principle, but the wrong tool for this
repo's deployment model.** A `.my` reader is exactly what `my-lisp`
itself already implements authoritatively (and exposes live via the TCP
oracle's `parse` op). But `cml` is a standalone AOT CLI/library with no
runtime `my-lisp` dependency — that's load-bearing for its CI story (no
live process needed to compile) and its use as a library. Delegating
parsing to `my-lisp` would mean either a live process dependency
(breaks the standalone-binary story) or embedding `my-lisp` itself as a
dependency (which is Rust anyway — porting `cml`'s reader to `my-lisp`
source wouldn't even remove a Rust dependency, just move which Rust
parser is in the loop). Not worth pursuing.

**`ir.rs`/`compiler.rs` — not migration candidates, by design.** These
are `cml`'s version of fpga-lisp's "operates hardware" exclusion: `ir.rs`
is the typed contract every current and future backend shares (its
whole reason to exist per `docs/heterogeneous-backends.md` is to be a
stable, checkable interface — untyped data would defeat that purpose),
and `compiler.rs` is the register-allocation layer where `docs/abi.md`'s
"one rule that matters" lives. Rust's exhaustive `match` over `Ir`'s
variants is doing real correctness work here (a missing arm is a
compile error, not a silent miscompile) — the same category of
protection static types gave `lower.rs`'s rewrite this session (catching
the string-literal lowering bug at the type level, not by re-deriving
register discipline in an untyped host).

## Priority summary

1. **`macros.rs` → a `.my`-hosted macro-expander**: real candidate,
   worth scoping as an actual task. Never touches fpga-lisp or register
   allocation, so the risk profile matches `assembler.my`'s (a bug there
   is a compile-time-only regression, not a hardware-verified-path
   regression). Blocked on nothing external, unlike `assembler.my`
   (which is blocked by a `my-lisp` interpreter bug per fpga-lisp's own
   doc) -- this could start now if prioritized.
2. **`lower.rs` → Lisp**: plausible later, not now. Only makes sense
   paired with a real Rust↔Lisp data boundary for `Ir`, which doesn't
   exist and isn't otherwise needed yet.
3. **`parser.rs` → Lisp**: not planned. `my-lisp`'s own reader is
   already the authoritative one; `cml` reimplementing it in Rust is
   about deployment independence (no live process/embedded interpreter
   dependency), not a language-suitability question.
4. **`ir.rs`/`compiler.rs` → Lisp**: not planned, not desired. Same
   reasoning fpga-lisp's `upload.py`/`monitor.py` verdict used, applied
   to a different kind of "hardware": if a future agent proposes this,
   point here first rather than re-deriving why it's a bad trade.
