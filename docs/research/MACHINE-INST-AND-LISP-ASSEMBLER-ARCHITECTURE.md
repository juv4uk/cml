# MachineInst Architecture and Lisp Assembler Substrate

**Status:** Canonical Architecture and Research Record  
**Date:** 2026-09-14  
**Relevant Issues:** #35, #36, #37, #38, #39, #40  
**Ukrainian Equivalent:** [MACHINE-INST-AND-LISP-ASSEMBLER-ARCHITECTURE.uk.md](file:///home/agents/GitHub/cml/docs/research/MACHINE-INST-AND-LISP-ASSEMBLER-ARCHITECTURE.uk.md)  

---

## 1. Epistemology & Authority Boundaries

The foundational architectural principle between the language and the compiler middle-end:

```text
my-lisp (language authority, semantic identities, canon definitions)
   │
   ▼  (direction: semantic -> compiler/target)
cml (compiler authority, optimization, placement, target code generation)
   │
   ▼  (target x86 facts: registers, displacements, bytes)
Physical CPU / FPGA
```

### Invariants:
1. **Reverse authority is strictly forbidden:** the compiler machine layer never invents or self-assigns language semantic identities.
2. **Semantic ID 1153 is permanently retired:** associating hardware `RDTSC` with semantic ID 1153 was an authority boundary error. ID 1153 is burned and excised from all contracts. Machine instructions are compiler-owned target mechanisms (`semantic_id = None`).
3. **CML is mechanism, not a second semantic layer:** `MachineInst` does not replace Lisp semantics with handwritten Rust. It is a target substrate for physical facts.

---

## 2. Proven Vertical Execution Slice (#36)

The vertical slice proves the entire pipeline from source expression to silicon execution without intermediate foreign runtimes:

```text
Lisp source: (+ 10 32)
   │
   ▼ (src/parser.rs)
AST Expr: (List [Symbol("+"), Integer(10), Integer(32)])
   │
   ▼ (src/lower.rs)
IR: PrimOp::Add
   │
   ▼ (src/machine_inst.rs::select_arithmetic_slice)
MachineItem:
   - MovImm64 (rax, 10)
   - AluImm32 (Add, rax, 32)
   - ShlImm   (rax, 3) | OrImm8 (rax, 1)  [fixnum tagging]
   - Syscall  (sys_write 8-byte word to stdout)
   - MovImm64 (rax, 60), Syscall (sys_exit 42)
   │
   ▼ (src/machine_inst.rs::assemble_program)
Raw x86-64 machine code bytes
   │
   ▼ (src/elf64.rs::Elf64Executable)
Standalone Linux ELF64 binary (PT_LOAD RX)
   │
   ▼ (native kernel execution)
Result: 42 (stdout + process exit code)
```

### Verification Evidence:
- **Zero C/Rust runtime:** no `libc`, `crt0`, `libgcc` or foreign libraries linked.
- **Determinism:** byte generation is deterministic across repeated runs.
- **Differential oracle:** bytes match GNU `as` + GNU `ld` output byte-for-byte.
- **Oracle agreement:** `my-lisp` CLI and `cml` C-backend return 42 on the same source fixture.

---

## 3. Machine Lowering Authority Boundary Contract (#38)

Formally verified against `contracts/machine-lowering-boundary.lisp` through `src/machine_boundary.rs`:
- `semantic-authority = my-lisp`
- `compiler-authority = cml`
- `lowering-direction = semantic-to-machine`
- `reverse-authority = forbidden`
- `retired-semantic-id = 1153`

---

## 4. Lisp-Authored Assembler Substrate & Macro-Atoms (#39)

Rather than hard-coding assembler macro patterns into Rust, **Lisp becomes its own assembler**:

```text
Symbolic layer: Lisp macro-atoms (contracts/machine-macro-atoms.lisp)
   │ (macro expansion in Lisp)
   ▼
Structured target data: (x86 ...) forms
   (x86 mov-imm64 rax 10)
   (x86 alu-imm32 add rax 32)
   (x86 tag-fixnum rax)
   │
   ▼ (src/machine_substrate.rs)
Structured MachineItem / MachineInst in Rust
   │
   ▼ (src/machine_inst.rs)
Direct physical byte encoding
```

### Macro-Atoms (`contracts/machine-macro-atoms.lisp`):
- `(tag-fixnum reg)` — tag fixnum via left-shift 3.
- `(untag-fixnum reg)` — untag via arithmetic right-shift 3.
- `(mov-imm reg imm)` — 64-bit immediate load.
- `(mov-reg dst src)` — register move.
- `(add-imm reg imm)` — 32-bit immediate addition.
- `(sub-imm reg imm)` — 32-bit immediate subtraction.
- `(fast-ret)` — fast procedure return.

These macro-atoms reside at the Lisp level, allowing users to build assembler libraries and idioms while CML remains the low-level byte-encoding mechanism.

---

## 5. Harvest & Branch Decomposition Plan (#37)

The monolithic research branch is harvested into modular, reviewable PRs onto `master`:

1. **PR #42 (`harvest/01-authority-repair-pin-refresh`)**: retire 1153, refresh `my-lisp` pin in CI, stabilize Guix test environment (#35, #40).
2. **PR 2 (`harvest/02-machine-inst-core`)**: `MachineInst` instruction model, registers, symmetric ALU, byte encoder, GNU `as` oracle.
3. **PR 3 (`harvest/03-label-resolution`)**: two-pass label resolution and rel32 branches.
4. **PR 4 (`harvest/04-elf64-synthesizer`)**: standalone Linux ELF64 synthesizer.
5. **PR 5 (`harvest/05-vertical-slice-witness`)**: end-to-end vertical witness `(+ 10 32) -> 42` (#36).
6. **PR 6 (`harvest/06-boundary-contract`)**: consume boundary contract (#38).
7. **PR 7 (`harvest/07-lisp-authored-assembler-substrate`)**: Lisp assembler substrate and macro-atoms (#39).
8. **PR 8 (`harvest/08-research-docs-consolidation`)**: architectural consolidation (#37).
