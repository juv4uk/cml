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

### Current semantic gate

`src/semantic.rs` is the first fail-closed admission pass between macro
expansion and lowering. It deliberately does **not** claim full my-lisp 3.0
semantic analysis. Its current executable boundary rejects two shapes that
CML previously could compile into a different program than the canonical
evaluator:

- duplicate lambda parameters, including collisions introduced by CML's
  current uppercase symbol representation;
- lambda bodies containing more than one expression, because canonical
  my-lisp evaluates them sequentially while the current `Ir::Lambda` can
  represent exactly one body expression.

Both `lower_program` and the public `lower_expr` entry point apply this gate,
so a source consumer cannot silently bypass it by selecting a backend. Further
contract coverage remains open work; rejection here is evidence of safe
non-support, not a claim that the full semantic-analysis milestone is done.

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

## Current Executable State (August 2026)

`cml` is a **multi-backend** heterogeneous compiler sharing one semantic IR:

- `src/parser.rs` → `src/ast.rs` → macro expansion → `src/lower.rs` → `ir::Ir` (`src/ir.rs`)
- Backends:
  - **FPGA (`src/compiler.rs`)**: fpga-lisp ISA, hardware-verified.
  - **C (`src/c_backend.rs`)**: Hosted C source, verified against the my-lisp oracle.
  - **x86_64 Freestanding (`src/x86_freestanding.rs`)**: Target ABI compatible GNU assembly for `wsm-os`, featuring bounded fail-closed semantics.
  - **CUDA (`src/gpu_cuda.rs` / `src/gpu_cuda_runtime.rs`)**: Emits and launches PTX compute kernels based on explicit execution graph analysis.
  - **WGPU (`src/gpu_wgsl.rs` / `src/gpu_wgpu_runtime.rs`)**: Portable WGSL compute backend (optional).
- `Ir` covers: literals, `nil`/`t`, variables, `quote`, `lambda`, application, `cond`, `let`, `def`, primitives, and new Compute representations (`Map`, `Reduce`, `Scan`, `Index`, `ParallelRegion`). It also features `Buffer` (I32/F32) and `TailSelfCall`.
- Neither backend supports rationals/bignums/inexact numbers yet (`compatibility.my`'s `limitations`).
- Backends explicitly fail-closed in a preflight validation step (`validate_ir`), meaning if an IR variant (like `Ir::Builtin`) is not supported, it is rejected with a typed `CompileError` rather than silently admitted and panicked upon.

The Execution Graph (in `src/execution*.rs`) now orchestrates these nodes, ensuring typed buffers correctly map between CPU host logic and GPU compute kernels, without claiming direct GPU-to-FPGA transfers.

## Historical Plan & Architecture Path (Preserved)

### Symbol ABI bridge (2026-08-24)

`Compiler::compile_with_symbols` assigns per-program `LOADSYM` IDs from 900.
The self-hosted `fpga-lisp/assembler.my` now implements the same normalization
and `.sym` sidecar contract. `tests/compiler_test.rs` includes a cross-repo
fixture that assembles CML output through both `assembler.py` and
`assembler.my` and requires identical bytes. This is an implementation-parity
proof only; it does not transfer language or backend authority to either
assembler. The compiler-test harness now prefers the release `my-lisp`
assembler and falls back to Python only when that binary is unavailable.

1. **Draw the backend boundary inside cml.** ✅ Done: a backend-neutral `Ir` covering every form `compile_*` in `compiler.rs` handles.
2. **C backend next, not CUDA.** ✅ Done: `src/c_backend.rs`, a small tagged-union `Value` runtime.
3. **Compute analysis after C.** ✅ Done: M0 implemented in `src/compute.rs`, GPU admission fails closed.
4. **Portable GPU backend after a typed-buffer contract.** ✅ Done: WGPU and CUDA backends implemented and integrated into the execution graph.
5. **fpga-lisp stays as a backend** of the same semantic IR, with a future dataflow lowering as a separate specialization path.

Later, the target can even be *chosen by the compiler* when provably safe, and a single program can span all three:

```lisp
(let ((raw (fpga-read)))
  (let ((processed (gpu-map transform raw)))
    (cpu-decide processed)))
```

## Swarm mapping

The four-repo swarm already maps onto this: `my-lisp` (language/semantic
source of truth), `cml` (compiler middle-end), `fpga-lisp` (FPGA
backend), `my-idea` (observatory). New execution backends (C, CUDA, x86_64) may
each become their own node in the P2P mesh — the mesh is designed for
new members to join with a single `--connect` (see my-lisp
`docs/swarm-mesh-v2.md`).

All backends are judged against the same semantic contract
(`language-contract.my` / `isa-contract.my` / `compatibility.my`), not
against each other's implementation.

## Non-goals for now

- No GPU in the build toolchain (Guix does not package the CUDA toolkit; nvcc stays a host-side Ubuntu tool).
- No change to fpga-lisp's contract or to `:9999` semantics.
