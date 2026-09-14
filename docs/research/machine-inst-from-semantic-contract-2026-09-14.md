# Research: derive `MachineInst` from my-lisp semantic authority

Date: 2026-09-14
Branch: `research/machine-inst-from-semantic-contract`
Base: `cml/master@a3dc96f5c0fe1371f3728c527606cdb6cb6b4db7`
Semantic research upstream: `juv4uk/my-lisp@research/machine-inst-semantic-contract`

## Purpose

CML should not invent language meaning. It should consume semantic identity from `my-lisp`, lower that meaning through explicit compiler passes, and only then choose target-specific machine instructions.

This branch is research only. No production compiler behavior changes here.

## Existing architecture is already close

Current CML already has the essential separation needed for the next step.

`repo.lisp` declares:

```text
role: compiler-middle-end
imports: language-semantics
owns: compiler-middle-end ir aot-compilation host-target
non-authority: language-semantics
```

Current `src/ir.rs` already defines:

```rust
pub enum MachineOp {
    Rdtsc,
}

pub enum Ir {
    ...
    MachinePrim {
        op: MachineOp,
        args: Vec<Ir>,
    },
    ...
}
```

Current `src/lower.rs` already dispatches from semantic ID `1153` to:

```text
Ir::MachinePrim(MachineOp::Rdtsc)
```

Current generated `contracts/cml-operations.lisp` already records:

```text
semantic-id: 1153
formal-action: machine:rdtsc
cml-ir-projection: Ir::MachinePrim(MachineOp::Rdtsc)
x86_freestanding: rdtsc
```

So the problem is not that CML lacks a machine boundary.
The problem is that the final backend currently jumps too quickly from IR to **textual GNU assembly strings**.

## Important authority problem found

Current CML head contains a fallback for machine primitive `1153` when building against a pinned `my-lisp` that does not yet provide the expected surfaces.

That is useful as an implementation bootstrap, but it is the wrong long-term authority direction.

Correct direction:

```text
my-lisp semantic registry / admitted contract
        |
        | authoritative semantic ID + observable meaning
        v
CML imports it
        |
        v
Ir::MachinePrim
```

Not:

```text
CML invents/patches missing semantic facts
        |
        v
pretends my-lisp already owns them
```

Long-term rule:

> If a machine-facing semantic identity is not admitted by the pinned my-lisp contract, CML must fail closed rather than manufacture language authority locally.

Fallbacks may exist only as explicitly temporary bootstrap code with an expiry condition.

## The next missing layer

Current path is approximately:

```text
source
  -> AST
  -> semantic-ID dispatch
  -> backend-neutral Ir
  -> x86_freestanding string emitter
  -> GNU assembly text
  -> external assembler
```

The research target is:

```text
source
  -> AST
  -> semantic-ID dispatch
  -> backend-neutral Ir
  -> target-selection pass
  -> canonical MachineInst data
  -> projections:
       +-> validator
       +-> asm printer
       +-> direct encoder
       +-> future decoder/disassembler
  -> machine bytes
```

The key idea is that **assembly text is not the target identity**.
It is one projection of structured instruction data.

## Proposed authority split

### my-lisp owns

- language meaning;
- numeric semantic IDs;
- observable results and ErrorKind behavior;
- surface-independent identity;
- semantic fixtures/oracle;
- admission of machine-facing capabilities when they become language-visible.

### CML owns

- lowering passes;
- target-neutral compiler IR;
- target operation selection;
- `MachineInst` representation;
- register constraints/selection;
- instruction declarations;
- encoder/printer/validator generation;
- byte-level correctness tests;
- ABI realization.

### x86 backend owns

- x86-64 instruction forms;
- registers;
- REX/ModR/M/SIB;
- displacement/immediate widths;
- relocation/fixup encoding;
- actual bytes.

No layer should silently become authoritative over the layer above it.

## Preserve `MachinePrim`; do not confuse it with `MachineInst`

Current `MachineOp::Rdtsc` is useful and should not be discarded merely because a new instruction layer is introduced.

The distinction should be explicit:

```text
MachinePrim / MachineOp
    target-neutral compiler operation
    e.g. ReadCycleCounter

MachineInst
    target-specific instruction identity
    e.g. x86_64 RDTSC
```

For x86 today the lowering may be one-to-one:

```text
MachineOp::Rdtsc
     -> MachineInst::Rdtsc
```

But the architecture must permit one operation to lower to many instructions:

```text
MachineOp X
     -> MachineInst A
     -> MachineInst B
     -> MachineInst C
```

and different targets to choose different sequences.

This keeps semantic/compiler operations separate from ISA identity.

## `MachineInst` should be structured data

Illustrative shape only:

```text
MachineInst {
  opcode-identity,
  operands,
  width/mode,
  optional relocation/fixup,
  source/provenance metadata
}
```

Examples conceptually:

```lisp
(machine-inst rdtsc)
(machine-inst mov (reg rax) (reg rbx))
(machine-inst add (reg rax) (imm 1))
```

The exact representation should be chosen to fit CML; the important invariant is that it is **data**, not a preformatted assembly string.

## One instruction declaration, many projections

The strongest idea from LLVM TableGen, SBCL instruction formats, Mezzano LAP and the ongoing WSM research is to avoid duplicating instruction knowledge across independent handwritten code paths.

Desired architecture:

```text
instruction declaration
      |
      +--> operand validator
      +--> encoding rule
      +--> asm printer
      +--> docs/test-vector generation
      +--> decoder/disassembler later
```

Illustrative declarative form:

```lisp
(define-machine-instruction rdtsc
  (target x86-64)
  (operands ())
  (encoding (#x0f #x31))
  (effects (writes rax rdx flags-none)))
```

or for a register operation:

```lisp
(define-machine-instruction mov-r64-r64
  (target x86-64)
  (operands ((dst r64) (src r64)))
  (encoding (rex-w modrm ...)))
```

These are research sketches, not proposed syntax.

## Why declarations beat handwritten opcode jungle

Without a declarative source of truth, x86 knowledge tends to drift into many places:

```text
validator knows one set of rules
printer knows another
encoder knows another
optimizer assumes another
```

That creates the same category of identity bug that `my-lisp` solved at the language level with numeric semantic IDs.

CML should make machine identity data-driven as well.

## Transitional strategy: keep the existing asm printer

Do not delete the current `x86_freestanding` emitter immediately.

Instead turn textual assembly into a transitional projection:

```text
Ir
  -> MachineInst list
       |
       +-> GNU asm printer  (existing execution path, initially)
       +-> validator
       +-> direct byte encoder  (new path)
```

This gives a powerful differential proof:

```text
same MachineInst
    |
    +-> printer -> GNU as -> bytes A
    +-> direct encoder      -> bytes B

compare decoded/equivalent instruction stream
```

Once the direct encoder proves itself, external assembly becomes oracle/tooling rather than production necessity.

## `rdtsc` is the ideal first vertical slice

CML already has:

```text
semantic ID 1153
  -> MachineOp::Rdtsc
  -> x86_freestanding backend
```

This makes `rdtsc` the perfect architecture probe because:

- it has zero operands;
- the x86 encoding is tiny;
- the semantic/machine boundary is obvious;
- there is no register allocator problem to hide the design;
- byte equality can be independently verified;
- native execution provides an observable witness.

First compiler research proof should therefore be:

```text
Ir::MachinePrim(MachineOp::Rdtsc)
       |
       v
[target-selection pass]
       |
       v
MachineInst(Rdtsc)
       |                 |
       |                 +-> printer -> "rdtsc"
       |
       +-> encoder -> 0F 31
```

Then verify both paths execute under the same existing witness harness.

Important: this compiler proof must be gated on `my-lisp` actually admitting the semantic identity; no local fallback should be considered semantic evidence.

## Second slice should force real x86 encoding structure

`rdtsc` alone does not test REX/ModR/M/SIB.

After the architecture works for `rdtsc`, choose one **existing CML operation** whose current x86 lowering naturally requires register operands.

The next proof should exercise, minimally:

```text
register identity
REX
ModR/M
one immediate or displacement form
```

Do not add arbitrary demo instructions merely to grow the encoder.
Every instruction admitted should be demanded by an existing semantic/compiler witness.

## MachineInst provenance

A useful field for debugging and scientific evidence is provenance:

```text
semantic ID(s)
source fixture or source span
lowering pass that selected this instruction
```

This is not semantic authority. It is evidence metadata.

It lets us answer:

```text
Why does this machine instruction exist?
Which Lisp semantic identity caused it?
Which lowering pass selected it?
```

That will be extremely valuable when comparing native execution against the my-lisp oracle.

## Nanopass-style compiler decomposition

Current CML already has a meaningful `AST -> Ir` boundary.
Do not replace that with a new monolith.

Preferred growth:

```text
AST
  -> semantic/backend-neutral Ir
  -> normalized machine-neutral Ir (only if evidence requires it)
  -> target-selected operations
  -> MachineInst
  -> encoded bytes
```

Each pass should be separately testable.

YAGNI rule:

> Do not add a new IR level unless one current transformation cannot be stated cleanly at an existing boundary.

The goal is Nanopass discipline, not Nanopass ceremony.

## Surface independence must survive all the way down

Current CML already dispatches Canon callables through semantic IDs rather than only source spellings.
The new machine layer must preserve that rule.

There must never be target code like:

```text
if source_name == "такти-процесора" ...
if source_name == "rdtsc" ...
```

The compiler lowering key is semantic identity / compiler operation.
Human surfaces stop mattering after semantic resolution.

## Error/effect semantics

Machine-facing operations are likely to expose volatility, capability restrictions, traps and nondeterministic values.
CML must not invent those semantics itself.

The compiler may represent them for scheduling/optimization, but the observable contract must come from my-lisp.

For example, the optimizer must know that cycle-counter reads cannot simply be common-subexpression-eliminated as if pure:

```text
(rdtsc) ; observation 1
(rdtsc) ; observation 2
```

Two calls are not semantically equivalent to one cached call.

Therefore an eventual compiler operation descriptor may need effect metadata imported or derived from semantic contract:

```text
pure
volatile-read
memory-read
memory-write
io-read
io-write
barrier
capability-gated
```

The exact vocabulary must be driven by real admitted operations.

## What should NOT be done on the next implementation branch

Do not:

- rewrite all of `x86_freestanding.rs` at once;
- build a complete x86 assembler;
- import Mezzano/SBCL opcode tables wholesale;
- add hundreds of instructions without semantic demand;
- expose x86 mnemonics to my-lisp Canon;
- let `MachineInst` contain EN/UK/UKR/SA spellings;
- delete the existing GNU asm path before differential proof;
- keep semantic-ID fallbacks forever;
- make byte equality the only semantic proof.

## First implementation milestone after research

Once the upstream my-lisp semantic boundary is accepted:

```text
Milestone M1

Input:
  Ir::MachinePrim(MachineOp::Rdtsc)

New pass:
  Ir/MachineOp -> Vec<MachineInst>

MachineInst declaration:
  x86-64 RDTSC, zero operands

Projection A:
  existing asm text printer

Projection B:
  direct byte encoder

Tests:
  1. semantic-ID provenance preserved
  2. MachineInst is source-spelling independent
  3. printer emits expected instruction
  4. direct bytes match external assembler/oracle
  5. native execution witness remains valid
  6. compiler fails closed if pinned my-lisp lacks admitted semantic identity
```

This is deliberately tiny.

## Second implementation milestone

Choose one existing operation requiring ordinary x86 operand encoding.
Add only enough declarative machinery to prove:

```text
register -> register number
REX
ModR/M
operand validation
printer and encoder derived from the same instruction declaration
```

If this stays small and clear, continue incrementally.
If it becomes a general assembler framework before the second semantic witness, stop and simplify.

## Relationship to direct Lisp-native encoding

Long-term ecosystem goal may be for the encoder itself to live in Lisp rather than Rust/C.
The `MachineInst` architecture helps rather than blocks that transition:

```text
semantic identity
  -> stable compiler operation
  -> stable MachineInst data
       |
       +-> today's Rust prototype encoder
       +-> future Lisp encoder
```

The crucial thing to stabilize first is the **data/contract boundary**, not the implementation language of the first encoder.

Once the representation and executable proofs are solid, moving encoder logic into Lisp is a bounded translation of mechanism instead of another semantic redesign.

## Research conclusion

CML is closer to the desired architecture than expected.

Keep:

```text
semantic-ID dispatch
backend-neutral Ir
MachinePrim / MachineOp
existing native witness infrastructure
```

Add:

```text
explicit target-selection pass
canonical structured MachineInst
single-source instruction declarations
printer + validator + direct encoder projections
```

Remove over time:

```text
CML-local semantic fallbacks
IR -> handwritten assembly-string coupling
repeated machine-operation facts across unrelated code paths
```

Authority direction must be:

```text
my-lisp meaning
    -> CML lowering
    -> MachineInst identity
    -> bytes
```

not the reverse.

The next implementation work should start only after the my-lisp semantic research decides how machine-facing observable effects/capabilities are admitted and exported.
