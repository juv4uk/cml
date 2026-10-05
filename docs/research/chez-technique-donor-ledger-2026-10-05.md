# Chez Scheme technique-donor ledger — 2026-10-05

Issue: #456

Purpose: extract independently testable compiler techniques from Chez Scheme without importing Chez runtime semantics, object identity, or implementation dependencies into CML/SENS.

Primary source used for this slice:
- Chez Scheme `IMPLEMENTATION.md`, current `main`, retrieved 2026-10-05:
  https://github.com/cisco/ChezScheme/blob/main/IMPLEMENTATION.md
- Chez Scheme backend implementation notes:
  https://github.com/cisco/ChezScheme/wiki/Backend-implementation
- Nanopass framework project:
  https://github.com/nanopass/nanopass-framework-scheme

The source audit below is about algorithms and boundaries, not authority. Chez behavior is never evidence for SENS meaning.

## 1. Nanopass organization

### Source/provenance

Chez documents a pipeline from expanded `Lsrc` through many explicitly named intermediate languages/passes in `cpnanopass.ss` / `np-language.ss`. Primitive calls become progressively more explicit forms before target instruction selection.

### Problem it solves

Large semantic jumps are difficult to debug, benchmark and prove. Small typed transforms make it possible to state what changed at one boundary instead of treating "the compiler" as one opaque transform.

### Minimal abstract algorithm

For an ordered compiler pipeline:

```
IR_0 --P_0--> IR_1 --P_1--> ... --P_n--> IR_n
```

give every `P_i`:
- a stable identifier/version;
- explicit input/output schema;
- a bounded admission predicate;
- a preservation obligation;
- replayable evidence.

### CML/SENS preconditions

CML already has this direction in #453:
- stable pass manifest;
- local proof obligations;
- upstream SENS oracle/case transport.

Do **not** import the Nanopass framework as a production dependency. The useful donor technique is the decomposition discipline.

### Semantic risks

- treating finite pass tests as a proof for all programs;
- allowing a target pass to mint SENS meaning;
- letting pass fusion erase the separately testable preservation obligation.

### Smallest local prototype

Already present: #453 pass registry + evidence transport.

Next bounded prototype after exact-domain transport lands:
- add `ir-to-slot.lower` as a named pass;
- feed one validated SENS case through it;
- record input/output artifact digests and oracle equality.

### Benchmark/proof obligation

For every active pass:
```
observable(interpret_out(P_i(x))) == observable(interpret_in(x))
```
under a declared bound/precondition.

### Decision

**ADOPT (already underway).**

Borrow the decomposition/evidence discipline. Do not import Chez's language definitions or runtime.

---

## 2. Register allocation: explicit constrained temporaries

### Source/provenance

Chez documents four allocator-visible categories:
- real registers;
- ordinary virtual variables/temporaries;
- unspillable temporaries that must receive a real register;
- pre-colored unspillables that must receive a specific register.

It also documents live-range construction from first assignment to last use, spilling ordinary temporaries before instruction selection, and retrying allocation when backend-created unspillables increase pressure.

### Problem it solves

Some machine instructions impose physical constraints that ordinary "choose any GPR" allocation cannot express cleanly.

CML already has deterministic liveness + linear-scan allocation in `src/x86_regalloc.rs`. Replacing it with Chez's allocator would add complexity without evidence.

A concrete CML pressure point already exists:
- x86 `DIV` consumes `RDX:RAX` and writes quotient/remainder to `RAX/RDX`;
- current machine instruction code knows this constraint;
- the general allocator does not expose a typed "must be RAX/RDX" temporary category.

### Minimal abstract algorithm

Keep the current allocator. Extend its input constraints with:

```
AnyReg(v)
MustReg(v, RAX)
MustReg(v, RDX)
Unspillable(v)
```

Allocation remains deterministic. A constraint conflict is a named allocation error, not silent register rewriting.

### CML/SENS preconditions

- constraint objects live only in x86/backend-local IR;
- shared CML IR and SENS identities remain register-free;
- constrained allocation must preserve the same machine-observable result as the current explicit/manual path.

### Semantic risks

- leaking ABI/ISA registers upward into canonical IR;
- converting an impossible allocation into a semantic fallback;
- silently clobbering values when two live constrained values require one physical register.

### Smallest local prototype

Use one already constrained instruction family, preferably x86 divide:
1. introduce backend-local fixed-register constraints;
2. lower one existing divide witness through the allocator;
3. prove the same encoded machine behavior/result;
4. add a negative overlapping-live-range conflict.

### Benchmark/proof obligation

Correctness first:
- byte/behavior parity with the current known-good divide sequence;
- no extra semantic dispatch.

Then measure:
- moves inserted;
- spills;
- instruction count;
- code size.

### Decision

**ADOPT AS A BOUNDED FOLLOW-UP, NOT A NEW ALLOCATOR.**

Keep CML linear scan. Borrow only the explicit constrained-temporary idea. Implementation child: #523.

---

## 3. Runtime representation/tagging

### Source/provenance

Chez documents a runtime where low pointer bits identify broad object classes and some pointed-to objects contain additional type words. The compiler and C runtime share these layout constants.

### Problem it solves

Compact runtime type tests and direct field addressing.

### Why this does not transfer as a language rule

This is exactly the class of design that #455 must keep *below* the target-neutral exact-Q representation boundary.

Chez's layout is tied to:
- pointer width/alignment;
- its GC;
- its C kernel;
- its object model;
- architecture-specific offsets.

None of those are SENS semantics.

### Minimal abstract algorithm worth retaining

Only the separation principle:

```
semantic exact value
  -> target-neutral representation class
  -> backend-specific encoding/layout
```

The backend may choose a tagged immediate, boxed object, limb representation, or another exact form, but must prove round-trip exactness.

### CML/SENS preconditions

Use #455 law:
```
decode_rep(encode_rep(q)) == q
```

Each target supplies its own admissible range/layout.

### Semantic risks

- copying Chez tag values into CML IR;
- treating pointer layout as portable semantics;
- forcing x86, SLOT-VM and FPGA to share one physical representation.

### Smallest local prototype

#455 / exact-Q representation classification is the correct prototype:
- target-neutral immediate candidate vs exact fallback;
- no pointer/tag bits in the shared layer;
- per-target range later.

### Benchmark/proof obligation

Per target, independently:
- exact round trip;
- boundary promotion;
- allocation/load/store delta;
- conversion cost;
- code size;
- crossover.

### Decision

**REJECT CHEZ'S CONCRETE TAG LAYOUT; ADOPT ONLY THE LAYERING LESSON.**

---

## Result of this research slice

| Technique | Decision | CML action |
|---|---|---|
| Nanopass decomposition | ADOPT | Continue #453; future `ir-to-slot.lower` must be an explicit proof boundary |
| Register allocator wholesale | REJECT | Existing deterministic linear scan stays |
| Constrained/pre-colored temporaries | ADOPT bounded prototype | #523 — backend-local x86 fixed-register constraints |
| Chez pointer/tag layout | REJECT | Never promote it into canonical CML/SENS IR |
| Representation layering | ADOPT | Continue #455 target-neutral class -> target-specific layout |

The useful pattern across all three is the same: **borrow mechanisms only where CML can state an independent preservation law.**
