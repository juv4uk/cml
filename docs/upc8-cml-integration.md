# UPC з погляду CML: де має жити кодування між source, IR, image та FPGA

**Автор:** Manus AI  
**Дата:** 18 серпня 2026 року  
**Статус:** архітектурна рекомендація; не реалізована зміна CML

## Головний висновок

Для CML UPC — не просто ще одна library function. Це майже ідеальний case для того, щоб компілятор нарешті відрізнив **runtime Lisp data** від **typed static binary data**.

Поточний CML path already має правильний skeleton: `parse → macro-expand → lower to backend-neutral Ir → FPGA/C backend` [1] [2] [3]. Але quoted data today lower-иться до cons construction: `Quoted::List` becomes stack pushes, `CAR`/`CDR`, and a final `CONS` per item [4]. Це правильно для ordinary Lisp lists. Для static UPC corpus це precisely the wrong lowering.

> **CML має побачити UPC literal як compile-time data object, перевірити його once against versioned UPC contract, покласти bytes у data section/bank, а в executable code передати лише descriptor.**

Тоді один source meaning може lower-итися по-різному без semantic duplication:

```text
source UPC literal / profile reference
                │
                ▼
shared UPC validator + canonical encoder
                │
                ▼
common IR: UPC bank + descriptor reference
          ┌─────┴─────┐
          ▼           ▼
C target: static uint8_t[]      FPGA target: packed BRAM/image section
          │           │
          └──── same bytes + manifest + test vectors ────┘
```

## 1. Чому current CML lowering не можна застосувати до UPC corpus

`Ir` today describes integers, symbols, strings, quoted lists/dotted lists, lambdas, calls, conditionals, definitions and a closed primitive set [2]. `compiler.rs` builds quoted lists at runtime: for every element it saves the partial tail on R11, compiles the item, restores the tail through `CAR`/`CDR`, then allocates the final list cell with `CONS` [4].

Для direct UPC byte `0–255` current positive-integer lowering emits at least one `LOADI`; surrounding quoted-list construction emits four more instructions and allocates two cons cells per list item — one temporary R11 stack push and one final list cell [4].

| Статичний UPC corpus: 4,096 direct bytes | Поточне quoted-list lowering | Correct data-section lowering |
|---|---:|---:|
| Raw data | 4,096 bytes | 4,096 bytes |
| Мінімум emitted instructions | 20,480 | Small descriptor-access sequence, plus loader policy |
| Мінімум temporary/final `CONS` allocations | 8,192 | 0 for byte storage itself |
| Against current FPGA image/heap budget | 5× 4,096-word imem; 2× 4,096-cell heap | Dedicated packed store/section |

Це не criticism of the current compiler. CML emits correct Lisp construction because `Quoted::List` semantically is a Lisp list. The insight is that **a UPC bank is not a Lisp list**. It is a compact immutable binary object with separately named grammar, profile and assignment table.

Якщо змусити CML compile `(quote (0 23 66 ...))` as UPC, він не зможе відрізнити an ordinary user list from protocol payload. Це перетворить optimization на hidden semantic special case. Потрібна явна syntax/IR boundary.

## 2. Правильний compiler boundary: після macro expansion, до ordinary IR lowering

Live CML CLI currently does exactly this: parse source, run `MacroExpander`, call `lower::lower_program`, then pass `Vec<Ir>` to the FPGA `Compiler` [1]. UPC extraction/validation should sit **between expanded source and generic lowering**, або be integrated into lowering as a typed output.

```text
parse
  → macro-expand
  → resolve UPC literals and profile references
  → validate/canonicalize/extract UPC banks
  → lower ordinary language forms to Ir
  → backend emit
```

Macro expansion must stay first. A macro may legitimately generate a static UPC literal; extracting earlier would make macro-produced data invisible or force two separate compile-time semantics. But after expansion, a UPC literal needs a stricter contract than `Quoted::List`.

| Phase | UPC responsibility | Error class |
|---|---|---|
| Parse | Recognize explicit UPC source form / literal syntax. | Syntax error. |
| Macro expansion | Produce final source form before canonical inspection. | Macro error. |
| UPC resolve pass | Find profile, assignment table and manifest; canonicalize source representation. | `unknown-profile`, `unsupported-version`, noncanonical input. |
| UPC validation | Check direct/extension grammar, length and no truncated sequence. | Compile-time UPC data error. |
| Common IR build | Intern verified immutable bank once; replace literal use with `DataRef`. | Internal/compiler error only. |
| Backend lowering | Emit target-specific descriptor / data section. | Target capability or resource error. |
| Runtime | Decode dynamic external data only; check actual device transport/integrity. | Runtime data error. |

This matches CML’s existing philosophy. Static arity and unknown-symbol errors are already classified in the frontend, while dynamic operand type errors travel back from the FPGA structured result channel [5]. A malformed literal UPC constant is analogous to static invalid arity, not to a runtime type error.

## 3. Proposed common data model

Do not add only `Ir::Upc(Vec<u8>)`; that loses the metadata which gives UPC its reproducibility. Instead introduce an immutable data section owned by the compiled module.

```rust
struct ModuleIr {
    code: Vec<Ir>,
    data: Vec<DataObject>,
}

enum DataObject {
    UpcBank {
        id: DataId,
        format_version: Version,
        assignment_version: Version,
        profile: ProfileRef,
        analysis_level: AnalysisLevel,
        bytes: Vec<u8>,
        decoded_unit_count: u32,
        boundary_sidecar: Option<Vec<u8>>,
        integrity: Option<IntegrityValue>,
    },
}

enum Ir {
    // current forms …
    DataRef(DataId),
}
```

This is illustrative Rust shape, not a requirement to name it exactly this way. Two architectural properties are load-bearing.

| Property | Why CML needs it |
|---|---|
| `DataRef` is separate from `Int`, `Quote`, `Var` and `Symbol` | Prevents byte payload from silently becoming a Lisp list or a symbol. |
| Module owns immutable `DataObject` bank | Enables literal interning/deduplication: equal UPC bank written twice need not double an image. |
| Manifest travels with bytes | Cross-backend output knows which grammar/profile makes those bytes meaningful. |
| `decoded_unit_count` is explicit | Aids bounds checking and optional sidecars without scanning at every use. |
| `boundary_sidecar` is optional | No cost for inventory workloads that do not need morphology/prosody. |
| `DataId` is not `SegmentId` | A bank/object identity is distinct from phonological semantic identity. |

`SegmentId` stays in the phonological registry and rules layer. `DataId` identifies an embedded compiled asset. The same `SegmentId` can occur in many banks; the same byte string can mean differently only if the manifest violates the contract, which validation must prevent.

## 4. Compile-time validation: exactly what CML should own

Compiler-time UPC checks have unusual value because the bytes are known before any FPGA/UART cost is paid. The compile-time resolver should reject the following before emitting a single instruction.

| Check | Why it belongs in CML |
|---|---|
| Unknown `format_version` or `assignment_version` | Compiler cannot knowingly emit data whose grammar it does not understand. |
| Unknown/mismatched profile revision | Source spelling without profile is not reproducible phonological data. |
| Noncanonical source representation | One declared literal must compile to one canonical byte sequence. |
| Truncated `E0`/`E1`/`E2` extension | Static bytes are provably malformed now. |
| Reserved prefix under UPC v1 | A known incompatible/undefined form must not become accidental data. |
| Invalid profile mapping or missing segment assignment | Prevents generated code from embedding an unrepresentable sound. |
| Static bank exceeds target data budget | Better diagnostic than a late imem/heap/BRAM failure. |
| Boundary sidecar length inconsistent with decoded unit count | This is a data invariant, not a runtime linguistic choice. |

The validator must not overreach. It should **not** claim to decide whether an external future corpus has valid phonology, whether a dynamic profile selection is semantically wise, or whether a research mapping is universally correct. Those are runtime/knowledge/provenance questions. CML only proves: “this declared literal obeys this declared versioned contract.”

## 5. Backend lowering should diverge only after common validation

CML already aims at a backend-neutral IR, and its C backend consumes the same `Ir` rather than a separate frontend [2] [6]. UPC is a chance to make that design materially useful.

### FPGA lowering

The FPGA backend should emit **two artifacts**, not try to smuggle raw bytes through executable `LOADI`/`CONS` code.

```text
1. Program image
   ordinary CML-generated instruction words

2. UPC data image / section
   manifest + descriptor table + packed bytes + optional sidecars
```

`DataRef(BANK_7)` should lower to the smallest target representation that is actually supported: perhaps a descriptor ID/offset, perhaps a pointer-like word after a shared ABI exists. It must not mean a pointer to the existing cons heap unless that was explicitly chosen and proven.

Current FPGA ISA has all primary opcode values allocated, while `IN`/`OUT` already expose byte-oriented I/O and the monitor is a separate binary protocol [7]. Therefore a CML UPC data section should not cause an opportunistic new opcode. The backend needs an agreed loader/bank interface first; only then may a small existing-path primitive or peripheral protocol expose `DataRef` to compiled code.

### C lowering

The C backend should emit the same bank as a `static const uint8_t[]` plus a descriptor struct, never as a nested `mk_cons(...)` expression. Its current runtime has a tagged union and heap-allocated cons values [6], so using static C storage gives both performance and a clean independent reference representation.

```c
static const uint8_t upc_bank_7[] = { 0x00, 0x17, 0x42, 0x11 };
static const struct upc_descriptor upc_desc_7 = {
    .format_version = …,
    .assignment_version = …,
    .profile_id = …,
    .byte_length = sizeof upc_bank_7,
    .decoded_unit_count = …,
};
```

The exact C ABI is deferred, but this generated shape makes one point testable immediately: the C target and FPGA target receive byte-for-byte identical canonical payload and manifest.

## 6. ABI: CML needs a third contract axis, not an implicit extension of ISA 1.0

CML already separates `my-lisp` language contract from `fpga-lisp` ISA contract in `compatibility.my`; the file correctly says compatibility is specific to declared contracts, not merely repo releases and SHA movement [8]. UPC should follow that rule.

```text
Compatibility tuple for a UPC-enabled artifact

(my-lisp language contract)
  + (CML IR/data-section contract)
  + (fpga-lisp ISA contract)
  + (UPC format contract)
  + (UPC assignment/profile revisions)
```

| Contract axis | Controls | Must not silently change |
|---|---|---|
| `language-contract` | Lisp semantics, reader/evaluator behavior | Meaning of ordinary source forms. |
| `cml-ir-contract` | `DataRef`, `DataObject`, literal extraction semantics | What a compiled UPC reference denotes. |
| `fpga-isa-contract` | Word/tag/register/loader behavior | Executability of program image. |
| `upc-format-contract` | Byte grammar, prefix lengths, canonical encoding | Parse of packed bytes. |
| `upc-assignment/profile` | Segment table and language mapping | Linguistic interpretation of valid bytes. |

This prevents a subtle but serious failure: CML v0.x and FPGA ISA 1.0 can remain compatible while a new UPC extension grammar changes the meaning of a bank. That is a separate compatibility event and must be declared separately.

## 7. Source syntax: make it explicit, but do not overdesign it

For source-level ergonomics, one could later introduce an explicit form such as:

```lisp
(upc-literal
  (profile sanskrit-phonemic-v1)
  (analysis phonemic)
  (source "kha"))

; or an explicitly serialised form
(upc-bytes
  (format upc-8-v1)
  (profile ukrainian-phonemic-v1)
  (hex "00174211"))
```

The exact syntax is not the priority. The priority is that it be **syntactically distinct from `(quote (0 23 66 17))`**. The first form is a typed compile-time declaration; the second remains ordinary Lisp data and must preserve ordinary list semantics.

Also do not let raw IAST/IPA become target syntax that CML itself resolves loosely. CML should consume a named profile and canonicalized mapping artifact; the richer human input pipeline belongs above it, in `my-lisp-panini`/semantic tooling or host-side profile compiler.

## 8. Cross-backend conformance should test bytes and semantics separately

The existing blind adapter is valuable because it drives shared fixtures through parse, macro expansion, lowering, assembly, UART simulation, heap decoding and canonical comparison without fixture-specific compiler branches [5]. UPC conformance should extend this style rather than add hand-written FPGA demos.

Three test layers are needed.

| Layer | Assertion | Evidence source |
|---|---|---|
| **UPC contract tests** | Same input/profile ⇒ canonical bytes; malformed extensions reject; unknown version blocks. | Shared UPC reference library and corpus vectors. |
| **CML extraction tests** | Source literal is extracted once into `DataObject`; ordinary quoted integer list is not. | Frontend/IR unit tests. |
| **Backend artifact tests** | C static array and FPGA data image carry identical manifest/hash/bytes. | Generated artifact comparison. |
| **FPGA E2E data-plane tests** | Packed bank decodes correct direct/extension units, detects length/prefix errors, crosses 32-bit word boundary correctly. | Icarus/SystemVerilog testbench + host oracle. |
| **Semantic integration tests** | A compiled query sees expected `SegmentId`/feature outcome via defined accessor, not raw bytes. | C and FPGA selected fixtures. |

Important distinction: the existing conformance decoder reconstructs Lisp words and cons heap into text [5]. A UPC bank should not be shoehorned into that heap dump. Give the testbench a deliberately named result channel: bank descriptor fields, byte count, optional checksum, decoder statuses and selected decoded units. This keeps the current Lisp heap canonical decoder clean and makes UPC evidence independently inspectable.

## 9. CML-specific optimizations worth doing

| Optimization | Value | Preconditions |
|---|---|---|
| Literal bank interning | Reuses equal canonical bytes across source sites. | Hash manifest + bytes after validation. |
| Compile-time source→UPC encoding | No runtime transliteration/parser cost for static literals. | Profile compiler is deterministic and versioned. |
| Generate direct-code feature masks | Converts common natural-class checks into compact tables. | Feature registry and stable direct code assignments. |
| Embed decoded-unit count | Avoids scanning a variable-length bank merely to size a loop. | Validator computes it once. |
| Pack bytes separately from instruction words | Prevents program-imem/UART bloat from list construction code. | Data image/loader contract. |
| Deduplicate profile manifests | Shared profile metadata is emitted once, not per sequence. | Profile IDs and revisions are explicit. |

Do **not** make semantic rules depend on a CML compile-time `u8` identity. CML can generate a feature mask for a direct UPC code, but the source registry must remain authoritative. The compiler is a consumer and packer of the contract, not a second hidden phonology database.

## 10. Practical staged plan

| Priority | Work | Why this order |
|---|---|---|
| **P0** | Write a small shared UPC contract library: canonical encode/decode/validate + manifest types + test vectors. | One source of truth before any compiler or RTL optimization. |
| **P0** | Add explicit UPC source form and frontend extraction pass after macro expansion. | Avoids conflating typed binary data with ordinary quoted lists. |
| **P0** | Extend CML module output from only `Vec<Ir>` to `code + immutable data objects`. | Creates the cross-backend architecture once. |
| **P0** | Add C backend static-array emission and CML unit tests. | Cheapest real proof that data section is not cons lowering. |
| **P1** | Define FPGA UPC data-image/loader/descriptor ABI alongside, not inside, ISA 1.0. | Protects existing opcode and machine-image contracts. |
| **P1** | Add blind artifact comparison and RTL bank-decoder E2E tests. | Turns compatibility into evidence instead of documentation. |
| **P1** | Add target-aware diagnostics for bank size, unsupported dynamic usage and profile/version mismatch. | CML should reject known impossible artifacts early. |
| **P2** | Introduce first-class `Bytes`/UPC descriptor tag only after my-lisp/CML/fpga-lisp agree on memory layout. | Avoids a target-only representation split. |

## Висновок

CML can make UPC much more than a compact byte table. It can turn a profile-backed phonological representation into a **portable, versioned compiled asset**.

The key move is simple:

> **Do not compile UPC as Lisp list construction. Compile it as validated static data, and compile only a typed reference to it as code.**

That preserves the separation your ecosystem already values:

```text
my-lisp / Panini tooling  → meaning, profiles, provenance
CML common layer          → validation, canonicalization, data-object IR
C backend                 → static bytes and independent reference execution
fpga-lisp                 → packed data bank and strict streaming decoder
```

It also gives you a clean conformance story: same source profile and same UPC contract must yield the same bytes in every backend; the hardware decoder must agree with the shared host oracle; and linguistic rules remain about `SegmentId`/features, not accidents of emitted byte layout.

## References

[1]: https://github.com/juv4uk/cml/blob/master/src/main.rs "CML live parse → macro-expand → lower → FPGA compiler pipeline"
[2]: https://github.com/juv4uk/cml/blob/master/src/ir.rs "CML backend-neutral IR and quoted data model"
[3]: https://github.com/juv4uk/cml/blob/master/src/lower.rs "CML AST to IR lowering"
[4]: https://github.com/juv4uk/cml/blob/master/src/compiler.rs "FPGA emitter and runtime quoted-list construction"
[5]: https://github.com/juv4uk/cml/blob/master/tests/conformance_test.rs "Blind CML → FPGA conformance adapter"
[6]: https://github.com/juv4uk/cml/blob/master/src/c_backend.rs "CML C backend and tagged-union runtime"
[7]: https://github.com/juv4uk/fpga-lisp/blob/master/isa-contract.my "fpga-lisp ISA contract 1.0"
[8]: https://github.com/juv4uk/cml/blob/master/compatibility.my "CML compatibility contract and declared limits"
