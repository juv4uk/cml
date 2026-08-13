# cml as a backend-independent compiler core

Status: strategy agreed 2026-08-12 (owner + opencode engineer); steps 1
and 2 of the incremental path below are now implemented (`ir.rs`/
`lower.rs`, `compiler.rs` consuming `Ir`, `src/c_backend.rs`) — see
"Current reality" for what actually exists today, not just the plan.
This file still exists so a "CUDA backend" (step 3, not started) can't
quietly become a second independent compiler once someone picks it up.

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

`cml` is a **two-backend** compiler sharing one IR:

- `src/parser.rs` → `src/ast.rs` → macro expansion → `src/lower.rs` →
  `ir::Ir` (`src/ir.rs`) → either `src/compiler.rs` (fpga-lisp ISA,
  hardware-verified, `docs/abi.md`'s register discipline) or
  `src/c_backend.rs` (C source, built and run with real `gcc`, verified
  against the live `my-lisp` oracle rather than by inspection).
- `Ir` covers: literals, `nil`/`t`, variables, `quote` (integers/
  symbols/lists/dotted lists), `lambda` (fixed, variadic, and
  bare-symbol param lists), application, `cond`, `let`, `def`
  (including self-recursive, via a letrec-placeholder-plus-backpatch
  pattern both backends implement independently -- `SETCDR` on
  fpga-lisp, a literal C struct-field mutation in `c_backend.rs`), and
  the primitives `cons`/`car`/`cdr`/`eq`/`atom`/`equal?`/`+`.
- Neither backend supports rationals/bignums/inexact numbers yet
  (`compatibility.my`'s `limitations`) -- `*` was never implemented on
  any backend either (the original `(def square (lambda (x) (* x x)))`
  acceptance example in step 2 below used it before that was noticed;
  the actually-verified fixtures use `+`).
- Long-term goal in `README.md`: run `unify.my`/`reason.my` fast on
  fpga-lisp hardware. That stays; it's the fpga-lisp backend's target,
  same as before the IR existed.

The C backend is real but intentionally narrower in one respect the
fpga-lisp backend isn't held to: it hasn't been exercised against the
full `tests/fixtures/conformance.my` suite the way `conformance_test.rs`
exercises `compiler.rs` (`ir_lowering_test.rs` only proves *lowering*
succeeds for every tier-1 fixture, not that `c_backend.rs` compiles and
runs each one correctly) -- worth closing before claiming real parity.

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
   ✅ Done (`a88970e`): `compiler.rs` rewritten form-for-form against
   `Ir`/`Params`/`PrimOp`/`Quoted` instead of `ast::Expr` -- same
   register sequences, same labels, same emit order. Zero external
   behavior change verified: full regression clean, and the real
   `length`/`length-onto` pair assembles to the identical 218
   instructions as before. Also fixed a real lowering bug this surfaced
   (a source string literal was wrongly lowering to a variable lookup
   instead of a `LOADSYM` literal). `Ir` is now the only thing
   `compiler.rs` sees -- `ast::Expr` never reaches code generation.
   Next: start the C backend.
2. **C backend next, not CUDA.** ✅ First increment done (`e7bc0df`):
   `src/c_backend.rs`, a small tagged-union `Value` runtime with a
   mutable-cons alist env, one C function per lambda, self-recursive
   `def` via the same letrec-placeholder-plus-backpatch idea
   `compile_def` uses on fpga-lisp. Verified against the real my-lisp
   oracle (not just internal consistency): `((lambda (x) (+ x 1)) 41)`
   and a self-recursive `(count 3)` both compile to C, build with real
   `gcc`, run, and match the oracle exactly (`tests/c_backend_test.rs`).
   Scoped down deliberately: fixed-arity lambda params only, no `let`,
   no quoted lists yet (documented in the module's own doc comment, not
   silently missing) -- the doc's own `(* x x)` example used a primitive
   (`*`) `cml` has never actually implemented on any backend, so the
   verified fixture uses `+` instead.
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
