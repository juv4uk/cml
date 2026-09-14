# Lisp-authored assembler: comparative design notes

Status: research branch only. No production semantics changed.

## Current CML position

CML already has two converging paths into one structured x86 layer:

1. Lisp source -> CML IR -> `MachineItem` -> direct encoder -> ELF64.
2. Lisp-authored machine forms / canonical Lisp macros -> `(x86 ...)` -> `MachineItem` -> the same direct encoder.

The important property is that the Lisp-authored layer does not allocate source-language semantic IDs and does not require host-side convenience primitives such as a Rust-owned `list`.

## Implementations inspected

### SBCL

Relevant source:
- `src/compiler/assem.lisp`
- `src/compiler/x86-64/insts.lisp`

SBCL keeps the assembler in Lisp and uses declarative macros such as `define-instruction-format` and `define-instruction`. Instruction definitions combine:

- operand/format description,
- printer support,
- direct byte emission,
- effective-address encoding,
- labels/fixups/backpatching.

Important lesson for CML: the mature step is not merely adding more mnemonics. It is separating reusable encoding forms (REX, ModR/M, SIB, immediates, displacement, fixups) from individual instruction names.

### Chez Scheme

Relevant source:
- `IMPLEMENTATION.md`
- `s/x86_64.ss`

Most of the Chez compiler is implemented in Scheme. Backend files define registers, instruction selection, and assembler operations. Chez emits machine code directly rather than depending on a system assembler.

A Chez `code` sequence is more than raw bytes: it can carry relocation entries and optional human-readable forms. This is the strongest architectural lesson for CML.

Important lesson for CML: before broadening the instruction set, introduce an explicit relocation/fixup representation between symbolic `MachineItem`s and final bytes.

### SpecOps

Relevant source:
- `phantomics/specops`

SpecOps treats instruction encoding as a declarative specification with operand classes, alternative encodings, provisions and priorities. This is useful for x86 because one mnemonic can have many physical encodings.

Important lesson for CML: a future x86 encoding table can replace large handcrafted `match` trees, but only after CML has stable typed operand classes. Do not copy the full SpecOps abstraction early.

### cl-asm

Relevant source:
- `Phibrizo/cl-asm`

`cl-asm` has a native `.lasm` frontend where mnemonics are Common Lisp functions and the full Common Lisp language can generate assembly. It also has an explicit IR, symbol table, linker, optimizers, disassemblers and multiple architecture backends.

Important lesson for CML: its infrastructure separation is valuable, but its use of the full host Common Lisp as the assembler macro language is intentionally *not* the CML direction. CML should preserve its smaller canonical Lisp meta-core so the assembler remains expressible by the language rather than borrowed from a host Lisp implementation.

### Yalo / small bootstrap Lisps

Projects such as Yalo and small Lisp-to-x86 compilers show the bootstrap direction clearly, but usually either depend on an external assembler or mix language runtime concerns with low-level assembly concerns.

Important lesson for CML: useful as bootstrap witnesses, but weaker architectural models than SBCL/Chez for the assembler substrate itself.

## What CML should keep

- `(x86 ...)` forms as target facts, not language semantics.
- `MachineItem` as structured data rather than assembly text.
- one encoder shared by compiler-generated and Lisp-authored machine programs.
- canonical Lisp macros built from the existing meta-core (`quote`, `cons`, etc.).
- GNU assembler only as an independent differential oracle, not a dependency of the direct-byte path.
- fail-closed parsing of immediate/displacement widths.

## Immediate correctness issue found in current master

`assemble_program()` currently computes symbolic label displacement as `isize` and then converts it using `disp as i32` for `jmp`, `jcc`, and `call`.

That means direct `(x86 ... rel32 ...)` input is range-checked, but symbolic label resolution can still silently truncate a displacement outside the signed 32-bit range.

This should become an explicit checked conversion before further assembler expansion.

## Recommended next architecture step

Do **not** start by adding dozens of x86 mnemonics.

Introduce a small target-mechanism layer:

```text
Lisp machine forms / compiler selection
              |
          MachineItem
              |
      symbolic Fixup/Reloc
              |
       encoding templates
              |
          machine bytes
```

Candidate types:

```text
RelocKind = Rel32 | Abs32 | Abs64
Fixup { at, target, addend, kind }
EncodedChunk { bytes, fixups }
```

Then make label resolution consume fixups with checked width conversion.

## Later, after relocation is explicit

1. Factor reusable x86 encoding shapes: REX, ModR/M, SIB, imm8/imm32/imm64, disp8/disp32.
2. Add typed operand classes instead of stringly instruction-specific parsing.
3. Consider a declarative machine encoding table inspired by SBCL/SpecOps.
4. Preserve a round-trip projection `(MachineItem <-> S-expression)` where meaningful.
5. Add sections/symbols only when ELF data or external linkage actually needs them.
6. Add branch relaxation only when there is a demonstrated need; fixed rel32 is simpler and deterministic today.

## Design guardrail

The assembler may become more data-driven, but the machine encoding table must not become a second source-language semantic table. Canon + the language function table still own language meaning; the machine table owns only physical target facts.
