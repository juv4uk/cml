# cml as a backend-independent compiler core

Status: strategy, agreed 2026-08-12 (owner + opencode engineer). Not yet
implemented — this file records the target and the incremental path to
it, so that a "C backend" or "CUDA backend" can't quietly become a second
independent compiler.

## The goal, stated once

> **my-lisp has one semantics and several physical forms of execution:
> CPU (C), GPU (CUDA), and FPGA (Verilog).**

`cml` is the middle-end for all three. The alternative — a my-lisp→C
compiler, a my-lisp→CUDA compiler, and a my-lisp→FPGA compiler as three
separate projects — is the anti-pattern this document exists to prevent.

## The whole picture

```
                my-lisp
                   │
             semantic IR
                   │
       ┌───────────┼───────────┐
       ▼           ▼           ▼
       C          CUDA       Verilog
       │           │           │
      CPU         GPU         FPGA
```

Compilation pipeline:

```
source → reader → macro expansion → semantic analysis → common IR
                                                          │
                                            ┌─────────────┼─────────────┐
                                            ▼             ▼             ▼
                                          C backend    CUDA backend   fpga-lisp backend
```

The one thing that makes all three backends shareable is a *common,
backend-neutral IR* in the middle. Everything below follows from having
that IR; nothing above it changes.

## Why purity is the enabler

my-lisp's functional, immutable semantics are the property that makes
this tractable:

- a pure function is trivially data-parallel → `(map square xs)` can
  become a CUDA kernel without alias analysis;
- a pure function is trivially a dataflow → `x → op A → op B → result`
  with no global-memory model, which is exactly what an FPGA wants;
- sequential / branching-heavy code stays on the CPU.

So the language's "encourages pure code" property is not a style bonus —
it is the architectural precondition for multi-target execution.

## Current reality (as of this commit)

`cml` is a **single-backend** compiler:

- `src/parser.rs` → `src/ast.rs` → `src/compiler.rs` → fpga-lisp ISA,
  emitted directly (`CONS`, `CAR`/`CDR`, `SETCDR`, `JF`, `RET`, ...).
- There is no IR module and no backend abstraction — the fpga-lisp ISA
  emission *is* the compiler. See `docs/abi.md`: every `compile_*`
  function hardcodes its registers.
- Long-term goal in `README.md`: run `unify.my`/`reason.my` fast on
  fpga-lisp hardware. That stays; it becomes one target of the IR.

So the fpga-lisp backend already exists in embryo. The work is to make
it *one of three*, not to write it from scratch.

## Incremental path (no rewrite)

1. **Draw the backend boundary inside cml.** ✅ Done (`src/ir.rs` +
   `src/lower.rs`): a backend-neutral `Ir` covering every form `compile_*`
   in `compiler.rs` handles (literals, `quote`, `cond`, `lambda`/variadic
   params, `let`, `def`, the seven primitives + `+`/`equal?`, application),
   plus `ast::Expr -> Ir` lowering. Verified: `tests/ir_lowering_test.rs`
   lowers every tier-1 conformance fixture and the real `length`/
   `length-onto` pair from `core.my` without error. **Additive only** —
   `compiler.rs`'s existing `ast::Expr -> fpga-lisp-ISA` path is
   untouched; nothing consumes `Ir` yet, so this step proves the
   boundary is well-defined without risking the hardware-verified path.
   Next: make `compiler.rs`'s fpga-lisp emission actually consume `Ir`
   instead of `ast::Expr` directly (still zero external behavior change),
   *then* start the C backend.
2. **C backend next, not CUDA.** C is the cheapest second target: the
   closure/env-chain lowering cml already does maps 1:1 onto C
   structures, and it validates the IR without any GPU involvement.
   A compiled-once `(def square (lambda (x) (* x x)))` → C → `cc`
   round-trip, compared against the my-lisp oracle, is the acceptance
   test for the IR.
3. **CUDA backend after C.** Only pure, element-wise forms (`map`/
   `fold`) become kernels; the compiler decides, or `with-target gpu`
   forces it. Not before the IR is stable.
4. **fpga-lisp stays as the third backend** of the same IR.

Later, the target can even be *chosen by the compiler* when provably
safe, and a single program can span all three:

```lisp
(let ((raw (fpga-read)))
  (let ((processed (gpu-map transform raw)))
    (cpu-decide processed)))
```

## Swarm mapping

The four-repo swarm already maps onto this: `my-lisp` (language/semantic
source of truth), `cml` (compiler middle-end), `fpga-lisp` (FPGA
backend), `my-idea` (observatory). New execution backends (C, CUDA) may
each become their own node in the P2P mesh — the mesh is designed for
new members to join with a single `--connect` (see my-lisp
`docs/swarm-mesh-v2.md`).

All backends are judged against the same semantic contract
(`language-contract.my` / `isa-contract.my` / `compatibility.my`), not
against each other's implementation.

## Non-goals for now

- No separate "my-lisp → CUDA compiler" project; no CUDA work until the
  IR and C backend exist.
- No GPU in the build toolchain (Guix does not package the CUDA toolkit;
  nvcc stays a host-side Ubuntu tool when/if CUDA work begins).
- No change to fpga-lisp's contract or to `:9999` semantics.
