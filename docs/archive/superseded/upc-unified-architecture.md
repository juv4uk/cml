# UPC: єдина архітектура від фонології до Lisp, компілятора й FPGA

**Автор:** Manus AI  
**Дата:** 18 серпня 2026 року  
**Статус:** зведена архітектурна пропозиція. Рекомендації нижче не є ратифікованою специфікацією, доки ти не зафіксуєш їх як versioned contracts у репозиторіях.

## 1. Виконавчий висновок

UPC має сенс не як спроба «вмістити всю фонетику людства в 256 значень». Така обіцянка була б науково хибною: phone, phoneme, transcription symbol, dialect description і фізична артикуляція — різні рівні, а навіть кількість фонем залежить від profile та аналітичного рішення [1] [2] [3] [4].

Натомість UPC може бути значно сильнішою річчю: **compact, versioned, profile-aware representation contract**, який має стабільне 8-bit ядро для frequent units, strict multi-byte extensions for rare/detail units, а всю семантичну, мовну й доказову складність тримає у відповідних вищих шарах.

> **UPC byte не є фонемою, profile не є мовою, а sequence не є proof.** Byte serializes a unit; profile explains a mapping; phonological IR names meaningful units; my-lisp stores claims and proofs; CML packs validated static data; FPGA streams compact bytes.

Це саме та дисципліна, яку вже видно в екосистемі. `my-lisp` розділяє semantic ID від SLP1 spelling, data-only knowledge import від executable `eval`, immutable World від глобальної пам’яті, а language contract від implementation release [5] [6] [7] [8]. `CML` already separates frontend semantics from backend lowering, while `fpga-lisp` separates small physical substrate from higher-level Lisp behavior through a versioned ISA contract [9] [10] [11]. UPC має вбудуватися в цей спосіб мислення, а не стати винятком із нього.

## 2. Архітектурна теза

```text
Human phonetics is open-ended and source-dependent.
        ↓
Profiles choose a bounded, documented interpretation for one task.
        ↓
Phonological IR gives stable identity and structure to that interpretation.
        ↓
UPC serializes it compactly with explicit grammar and versions.
        ↓
my-lisp reasons about claims, sources, rules, proofs and Worlds.
        ↓
CML validates static assets once and lowers them to target artifacts.
        ↓
FPGA stores and streams packed bytes without materializing Lisp lists.
```

The system remains extensible because each layer can evolve independently under a declared contract. It remains honest because no layer pretends to provide knowledge that belongs to another: FPGA does not decide a phonological analysis; CML does not become a hidden linguistics database; a profile does not redefine physical speech; raw bytes do not silently become truth.

## 3. Наукова межа: що UPC може й чого не може описувати

PHOIBLE 2.0 is a strong empirical reference point: it aggregates **3,020 inventories**, **3,183 segment types** and data from **2,186 distinct languages** [1]. But this is a curated, source-normalized sample, not a final set of every human speech sound. PHOIBLE explicitly works with **doculects**, because sources can legitimately differ for language varieties, periods, field methods and analyses [1] [2].

WALS illustrates the relevant typological range: its sample spans consonant inventories from **6 to 122**, and basic vowel-quality inventories from **2 to 14** [3] [4]. But those counts depend on decisions about affricates, length, diphthongs, nasalization and other segmentation questions. IPA is therefore a compositional notation system, not a fixed byte table of universal phonemes [12].

| Рівень | Приклад | Чи повинен UPC бути ним? |
|---|---|---|
| Articulatory possibility | Усюди можливі micro-variation of tongue, larynx, airflow | Ні. Це не finite engineering inventory. |
| Phone | `[k]`, `[kʰ]`, `[k̚]` | Частково, через extensions/modifiers when required. |
| Phoneme | `/k/` у конкретному profile | Так, як profile-backed `SegmentId` occurrence. |
| IPA/SLP1/IAST/orthography | Спосіб запису/вводу/показу | Ні як identity; це input and presentation layer. |
| UPC unit | `0x17` або `E1 02 7F` | Так, як compact representation only. |

### Архітектурне правило 1

> **UPC does not claim a final enumeration of human speech. It provides a versioned representation for selected phonological/phonetic units under named profiles and assignments.**

## 4. Поточне ядро та рекомендована граматика UPC v1

Current prototype inventory already demonstrates the intended shape: 42 Śiva Sūtras sounds occupy `0x00–0x29`; seven Sanskrit extensions occupy `0x2A–0x30`; Ukrainian work adds 31 new units through `0x4F`. This is **80 assigned direct values**, leaving space for current base evolution without pretending the direct table is universal.

For the first public binary grammar, the following partition is a practical proposal:

| Byte range / pattern | UPC v1 meaning | Why it exists |
|---|---|---|
| `00–DF` | Direct base unit | Frequent, stable units; preserves all current assignments. |
| `E0 xx` | One deterministic 2-byte extension | Rare segment or compact extension ID. |
| `E1 xx yy` | One deterministic 3-byte extension | Larger namespace or composed detail. |
| `E2 xx yy zz` | One deterministic 4-byte extension | Long-tail research/detail namespace. |
| `E3–FF` | Reserved and invalid in v1 | Future growth without reinterpretation of old data. |

The proposal leaves 144 additional direct positions after the existing 80 assignments, while the extension namespace grows much farther. Its central property is not capacity but parsing: **the first byte determines total unit length**.

```text
Direct     3A
EXT2       E0 91
EXT3       E1 02 7F
EXT4       E2 00 31 9C
```

`E0/E1/E2` are recommended illustration, not sacred values. What must become permanent before first data release is the invariant that direct assignments never later invade prefix space, and that each accepted sequence has exactly one canonical decoding.

### What not to inherit from historic text encodings

ASCII contributed enduring ideas — escape extension, logical separators, substitute/error awareness and explicit framing — but its printer and terminal controls are not UPC semantics. `CR`, `LF`, `TAB`, `BEL`, `FF`, `XON/XOFF`, device controls and text line conventions belong to presentation or transport layers, not to a phonological byte stream [13].

Moreover, UPC has already assigned low values as real sounds. `0x00` cannot become a magic `NUL`, and `0x0A` cannot become a magic newline merely because a C API expects it. ISO-8859 and Windows-1252 demonstrate why implied byte meanings are dangerous: the nominal C1 control range `0x80–0x9F` was repurposed by CP1252 as printable punctuation [14] [15]. A byte has UPC meaning only under an explicit UPC contract.

UTF-8 gives the right modern lesson: preserve stable values, make lead bytes determine length, reject invalid and overlong sequences, reserve impossible forms and make boundaries recoverable under known framing [16]. UPC should copy this **discipline**, not pretend to be a Unicode replacement.

## 5. The five independent contracts

No one version number can honestly describe the whole system. The same reasoning already motivates `my-lisp` language contract and `fpga-lisp` ISA contract separation [8] [11]. A UPC-enabled artifact needs at least five axes.

| Contract | Governs | Example incompatible event |
|---|---|---|
| **UPC format contract** | Byte grammar, prefixes, byte order, malformed policy | Reinterpreting a lead byte or changing extension length. |
| **UPC assignment contract** | Direct/extension ID ↔ `SegmentId` mapping | Reassigning a previously published code to another segment. |
| **Profile contract** | Source representation → phonological IR mapping | Changing how a token is segmented or normalized. |
| **my-lisp semantic contract** | Language, World, reasoning and data semantics | Changing what source program/knowledge statement means. |
| **Backend/ABI contract** | CML IR, FPGA ISA, data-bank descriptor and loader | Changing the physical meaning of a compiled `DataRef`. |

An implementation must declare the tuple it used, not hide it in a commit SHA:

```text
(upc-format        1.0)
(upc-assignment    1.0)
(profile           sanskrit-phonemic 1.0)
(analysis-level    phonemic)
(my-lisp-language  2.0)
(cml-ir-data       1.0)
(fpga-isa          1.0)
```

### Архітектурне правило 2

> **A compatible implementation may add new assignments or profiles, but it must never silently reinterpret a byte sequence valid under an earlier published contract.**

## 6. Шарова модель: від input до значення

The correct architecture has seven layers, each with a narrow responsibility.

| Layer | Primary object | Owns | Must not own |
|---|---|---|---|
| 1. Input/presentation | SLP1, IAST, IPA, Devanāgarī, orthography | Human entry, normalization, display. | Semantic identity or binary byte layout. |
| 2. Profile | Mapping rules and source provenance | Language/doculect analysis, tokenizer, scope, revision. | Universal truth about human phonetics. |
| 3. Segment registry | `SegmentId`, features, status | Stable identity, natural classes, aliases, evidence. | Per-occurrence context. |
| 4. Phonological IR | Ordered occurrences + boundaries/modifiers | Structure needed for sandhi, morphology, rules. | Hardware packing choices. |
| 5. UPC binary | Direct/extension byte units | Compact canonical serialization. | Proof, profile inference, display decisions. |
| 6. my-lisp knowledge/World | Facts, rules, journal, proofs, provenance | Reasoning and reproducible context. | Raw dataplane storage. |
| 7. CML/FPGA | Data sections, descriptors, BRAM stream | Compile-time packing and physical execution. | Linguistic interpretation independent of contracts. |

```text
SLP1 / IAST / IPA / orthography
                 │
                 ▼
      versioned language profile
                 │
                 ▼
 SegmentId + occurrence + boundary + modifier  ← phonological IR
                 │
                 ▼
   canonical UPC byte sequence + manifest
          ┌──────┴──────┐
          ▼             ▼
my-lisp: claims,      CML: typed static data
proofs, Worlds        ├── C static array
                       └── FPGA packed bank
```

### Segment, code unit, occurrence

These must never collapse into one type.

| Object | Example | Why it differs |
|---|---|---|
| `SegmentId` | `SEG_KH` | Stable phonological identity used by features/rules. |
| UPC code unit | `0x23` or `E1 02 7F` | Serialization under named assignment version. |
| Occurrence | Third unit of `SEQ_381`, before a morpheme boundary | Contextual instance that rules actually inspect. |
| Profile mapping | `"kh" → SEG_KH` under a named Sanskrit profile | Analysis-specific conversion, not identity. |

This directly continues your existing Sanskrit semantic architecture: `SLP1` is canonical source representation, IAST is presentation, and `Atom` identity must be independent of spelling [5] [6]. The same pattern applies here: **`SegmentId` is identity; UPC is representation; profile is interpretation.**

## 7. my-lisp: why UPC becomes more than a codec

`my-lisp` is the layer that can make UPC epistemically useful. `knowledge.my` accepts external information as data through guarded `advise`/`advise-all`, validates it, checks explicit conflict and appends it to an event journal; received knowledge is not passed to `eval` [7]. `world.my` makes a World an explicit immutable value with journal and metadata, and pure reasoning takes a World argument rather than inspecting hidden global state [6].

Thus a profile/mapping should not merely be a file that happens to exist. It should be a versioned, sourced assertion that can be named in a World.

```lisp
; Illustrative knowledge, not a prescribed API
(upc-profile sanskrit-phonemic-v1
  (format-version (1 0))
  (assignment-version (1 0))
  (normalization slp1-canonical-v1)
  (source source-shiva-sutras)
  (status experimental))

(segment-feature SEG_KH manner stop)
(segment-feature SEG_KH place velar)
(profile-maps sanskrit-phonemic-v1 "K" (SEG_KH))
(segment-code upc-base-v1 SEG_KH (35))
```

Then a proof can answer not only “what segment is this?” but also “under what profile, source, assignment version and World did this answer hold?” That is a major distinction from an ordinary codec.

### Data representation inside current my-lisp

Current runtime `Value` has text `String(Rc<str>)`, but no bytevector/raw-byte value [17]. Existing `read-file-bytes` and `write-file-bytes` already offer a practical P0 bridge: they read/write raw bytes as validated proper lists of exact values `0–255`, deliberately without UTF-8 conversion [18].

Therefore P0 can use a validated byte list inside my-lisp, but it must be treated as a **prototype data representation**, not a high-throughput storage model. Current TCP primitives are text-oriented and reject non-UTF-8 input, so raw UPC must not travel as `String` over that path [19]. Human-facing exchange can use hex/Base64 within a data-only S-expression envelope until a dedicated binary capability exists.

When corpus size, binary I/O or hardware integration make lists too expensive, introduce immutable `Bytes`/`ByteVector` as a distinct runtime type. It must be structurally equal by bytes, length-delimited, non-coercible to text, readable in canonical escaped/hex form and compatible with future CML/FPGA ABI only after a joint contract decision.

### Result and error semantics

`my-lisp` already has a reusable tagged-result convention distinguishing `unknown`, `partial`, `blocked` and `disputed` [20]. UPC should preserve this honesty.

| Condition | Meaning | Correct result |
|---|---|---|
| `malformed` | Physical sequence violates grammar. | Error with byte offset, raw bytes and format version. |
| `unsupported-extension` | Valid future namespace unknown to this decoder. | `blocked`/explicit unsupported result. |
| `unknown-segment` | Model/profile lacks a mapping. | Explicit unknown analysis with provenance. |
| `ambiguous-profile` | More than one valid profile applies. | `disputed` or explicit profile selection requirement. |
| `partial` | Bounded parse/search stopped before completion. | `partial`, never “no result”. |

Malformed data and incomplete linguistic knowledge are not the same event. The former is a format/transport failure; the latter is a claim about model coverage. They must not both turn into a generic substitute phoneme.

## 8. UPC frame and data-plane contract

Raw UPC payload should never be terminated by `NUL`, `LF` or a special final sound. These values are legal data now or may become legal data later. The enclosing container must carry length.

```text
UPC frame / bank descriptor
  magic                   optional format identifier
  upc_format_version      byte grammar
  assignment_version      code table
  profile_id + revision   mapping context
  analysis_level          phonemic / phonetic / experimental
  payload_byte_length     exact raw byte count
  decoded_unit_count      optional validated count
  flags                   optional boundary/integrity features
  payload                 exactly payload_byte_length bytes
  integrity               optional CRC/hash, container-defined
```

`NUL` can still serve as **physical padding outside a length-delimited payload**, for example in a fixed-width memory cell. But it can never be a magic terminator inside UPC data. Padding is a field/descriptor policy, not a property of byte `0x00`.

Logical `WORD`, `MORPHEME`, `SYLLABLE` and `UTTERANCE` boundaries are also not mandatory one-byte controls. They are optional typed annotations, added only when a rule needs them. For large FPGA banks, a sidecar indexed by decoded unit position is often cheaper than inline tokens.

| Boundary representation for 4,096 decoded units | Cost | Best use |
|---|---:|---|
| No boundary data | 0 bytes | Inventory/feature workloads. |
| One bit per position | 512 bytes | One yes/no boundary predicate. |
| Two-bit boundary kind | 1,024 bytes | Small mutually exclusive boundary set. |
| Four bitplanes | 2,048 bytes | Overlapping annotations. |

## 9. CML: compile meaning once, pack it per target

CML is the natural boundary at which static UPC data becomes a compiled asset. Its live pipeline is `parse → macro-expand → lower_program → FPGA compiler`, while the C backend consumes the same IR [9] [10] [21]. A UPC pass should run after macro expansion but before ordinary lowering.

```text
source
  → parse
  → macro expansion
  → UPC profile resolution + canonical validation + bank extraction
  → common IR (code + immutable data objects)
  ├── C backend: static const uint8_t[] + descriptor
  └── FPGA backend: packed data image + bank descriptor
```

The compiler must not treat ordinary `(quote (0 23 66 ...))` as UPC. That expression is an ordinary Lisp list, and current CML correctly lowers a quoted list to runtime `CONS` construction [21]. UPC must use an explicit source form and typed IR data object.

```rust
// Illustrative architecture, not committed CML API
struct ModuleIr {
    code: Vec<Ir>,
    data: Vec<DataObject>,
}

enum DataObject {
    UpcBank {
        id: DataId,
        manifest: UpcManifest,
        bytes: Vec<u8>,
        decoded_unit_count: u32,
        boundary_sidecar: Option<Vec<u8>>,
    },
}

enum Ir {
    // ordinary my-lisp forms
    DataRef(DataId),
}
```

### What CML validates

| Compile-time check | Reason |
|---|---|
| Known format and assignment version | The compiler must understand what it emits. |
| Known profile revision | Source mapping must be reproducible. |
| Canonical source form | One static declaration gives one byte sequence. |
| Valid direct/extension grammar | No `E1` tail truncation or reserved prefix reaches target. |
| Profile mapping completeness | No unrepresentable static segment is embedded. |
| Target data-bank budget | Fail before image/BRAM/UART cost. |
| Boundary sidecar size/count consistency | Prevent descriptor/data mismatch. |

This is aligned with CML’s current frontend/runtime split: syntax-visible errors are classified before compiling, while genuinely dynamic target failures surface through backend result channels [22].

### Why data sections matter

For 4,096 direct UPC byte literals, list construction needs a minimum of 20,480 emitted instructions and 8,192 `CONS` allocations in the current lowering pattern. That is five times current 4,096-word FPGA instruction memory and twice current 4,096-cell heap, whereas the raw bank is four kilobytes. The benefit is therefore not micro-optimization; it changes what artifacts are compilable.

`DataRef` should lower to a target descriptor, not a pointer into the Lisp cons heap. C can use a static `uint8_t[]`; FPGA can use a dedicated packed BRAM bank. Same manifest, same bytes, different physical layout.

## 10. FPGA: compact data plane, not expanded Lisp structure

`fpga-lisp` uses 32-bit tagged words and a physical cons heap of parallel 32-bit CAR/CDR BRAMs. A 4,096-element Lisp list thus requires 262,144 bits, while a packed UPC stream requires 32,768 bits: **an 8× structural storage advantage** [11] [23] [24].

The current GW5A-25A implementation uses 24 of 56 16-Kbit BSRAM blocks: eight for instruction memory and sixteen for heap. A 4 KiB packed UPC store is approximately two further blocks, moving the direct block count from 24 to 26, before exact vendor packing/routing effects are measured [23].

```text
Lisp heap                         UPC data plane
─────────                         ──────────────
closures / environments           packed byte BRAM
rules / proof terms / Worlds      descriptor table + manifest
ordinary lists                    feature masks / optional sidecars
```

### Streaming decoder

A sensible first datapath packs four UPC bytes into each 32-bit BRAM word. The decoder retains the current word plus a 2-bit lane selector, classifies the first byte and advances a small tail-count FSM only for extensions.

```text
byte fetch → 00–DF ? ─────────────→ emit direct unit
             E0 ?    ─────────────→ capture one tail, emit EXT2
             E1 ?    ─────────────→ capture two tails, emit EXT3
             E2 ?    ─────────────→ capture three tails, emit EXT4
             E3–FF ? ─────────────→ strict invalid/reserved result
```

The direct case is therefore a byte fetch, a compare/range classification and an optional feature lookup. It does not need a variable-bit entropy decoder, a linked-list traversal or a new ISA primary opcode.

### Natural classes as generated tables

Frequent feature predicates are cheap when expressed as generated direct-code masks. One 256-code bitset costs 32 bytes; sixteen natural-class masks cost 512 bytes. A `256 × 16-bit` feature ROM costs 4,096 bits; `256 × 32-bit` costs 8,192 bits. Extensions can take a slow fallback path.

The tables must be generated from the same versioned segment registry/assignment contract used by software. They may accelerate a claim such as “direct code belongs to class X,” but must never become a second, manually maintained phonology.

### FPGA restraint

Current ISA 1.0 has all 16 primary opcodes allocated [11]. UPC should not consume an opcode just to be “hardware-native.” `IN`/`OUT` already express byte-oriented boundaries; monitor features already use a separate binary protocol rather than opcodes [24]. A future UPC bank engine should be a data-plane/peripheral or contract-reviewed primitive boundary, not an implicit decoder hack in unrelated instruction fields.

## 11. Cross-layer verification strategy

The architecture is only credible if every boundary has a reference oracle and a visible status. This continues the established FPGA distinction between model evidence, RTL simulation, synthesis and hardware evidence [25].

| Evidence level | What it proves | Required UPC artifact |
|---|---|---|
| **Profile test** | Source form maps to intended `SegmentId` sequence. | Profile fixtures with provenance. |
| **UPC contract test** | Canonical encode/decode and malformed handling. | Host reference oracle and exhaustive vectors. |
| **my-lisp test** | Profile/assignment claim survives journal/World/proof path. | Data-only knowledge fixtures and proof output. |
| **CML extraction test** | Explicit UPC literal becomes one validated `DataObject`; ordinary list remains list. | IR assertions and diagnostics. |
| **Artifact equality test** | C/static and FPGA/data images contain same manifest + bytes. | Hash/byte-for-byte comparison. |
| **RTL simulation** | Packed-bank decoder handles direct/extensions, word boundaries and errors. | SystemVerilog bench plus host oracle. |
| **Synthesis** | Resource/timing claim is real on target device. | Vendor report: BRAM/LUT/FF/slack. |
| **Board test** | UART/load/runtime behavior works on physical hardware. | Reproducible acceptance script. |

### Known P0 correctness issue

Before publishing a UPC contract, fix the discovered pratyāhāra marker-collision defect in the prototype. The corrected exhaustive oracle examined 301 valid forms and found 81 mismatches, caused by skipping a listed sound when its SLP1 spelling matches the selected it-marker. The implementation fix is to remove the range-loop condition that discards `sound_slp1 == marker_slp1`, correct `hal` expectation from 32 to 33, and preserve the exhaustive oracle as a regression test.

This is architecturally important: a byte representation may be perfect while the linguistic generator feeding it is wrong. Verification must cover both the **semantic source** and the **binary target**.

## 12. Non-goals and anti-patterns

| Do not do this | Why it fails |
|---|---|
| Claim all human phonetics fits final 256 direct codes | Confuses an engineering base inventory with open-ended phonetic reality. |
| Treat `SegmentId == UPC code == SLP1 string` | Ties semantic meaning to representation and spelling. |
| Reclaim ASCII control bytes from low UPC values | Low values already carry data; controls belong in frame/grammar semantics. |
| Send raw UPC through UTF-8 `String` APIs | Arbitrary byte payload is not text. |
| Store corpus as a Lisp list on FPGA | Burns instruction image and cons heap; loses data-plane locality. |
| Add a new FPGA opcode prematurely | ISA is full and a data bank does not intrinsically need an opcode. |
| Make CML compile ordinary quoted number lists as UPC | Breaks ordinary Lisp semantics through an invisible special case. |
| Embed profile/version before every unit | Turns control metadata into permanent payload tax. |
| Add Huffman/RLE/entropy coding first | Variable-bit state, random access and recovery cost more than needed before measurements. |
| Silently substitute malformed/unknown data | Corrupts linguistic reasoning and provenance. |

## 13. Roadmap

The roadmap deliberately follows your principle: add a capability when a real need earns it, not because the final imagined system could use it someday.

### P0 — stabilize the research and binary contract

| Work | Deliverable |
|---|---|
| Fix pratyāhāra marker collision | Correct implementation + exhaustive 301-form regression suite. |
| Publish `upc-format-contract` | Direct/prefix ranges, byte order, canonicality, error taxonomy. |
| Publish `upc-assignment` registry | Stable `SegmentId ↔ code` mapping with status and evidence. |
| Define profile manifest | `profile_id`, revision, normalization, analysis level, source/provenance. |
| Host reference codec | Encode/decode/validate, hex exchange format, corpus profiler. |
| my-lisp `lib/upc.my` prototype | Validated list-of-bytes functions and structured statuses. |

**Success criterion:** the same profile/input deterministically produces the same canonical byte sequence, and malformed/unknown states are distinguishable without using FPGA or CML.

### P1 — create the compiled/data-plane path

| Work | Deliverable |
|---|---|
| CML explicit UPC literal | Syntax distinct from ordinary quoted list. |
| CML module data objects | `code + immutable data` rather than code-only lowering. |
| C static bank emitter | `static const uint8_t[]` plus manifest descriptor. |
| FPGA `upc_bank` experiment | Packed BRAM, descriptor, strict streaming decoder, no ISA opcode. |
| Generated feature masks | Direct-code natural-class ROM/bitsets from registry. |
| Artifact and RTL tests | Host/C/FPGA byte equality plus decoder vectors. |

**Success criterion:** a static UPC bank does not consume cons heap or generate list-construction code; C and FPGA receive identical canonical payloads.

### P2 — add only measured necessities

| Trigger | Capability to add |
|---|---|
| Large corpus/list overhead or binary host transport is a real bottleneck | Immutable `Bytes`/`ByteVector` in my-lisp. |
| FPGA programs query boundary-aware rules | Optional decoded-position boundary sidecars. |
| Need resident high-throughput bank access | Defined loader/peripheral interface and descriptor ABI. |
| Stable cross-repo memory model exists | CML/my-lisp/fpga-lisp shared `Bytes`/buffer tag and heap rules. |
| Measurements prove archive/UART bandwidth bottleneck after packing | Optional external compression with decompression to canonical UPC bank. |

**Success criterion:** each added feature has a measured workload, a contract update, a cross-backend test and an explicit epistemic status.

## 14. Final architectural position

The durable design is not “a universal phoneme byte table.” It is a set of mutually reinforcing contracts:

```text
Scientific humility
  → profiles name assumptions and sources

Semantic stability
  → SegmentId is independent of spelling and byte representation

Binary stability
  → direct core + deterministic extensions + strict decoding

Knowledge honesty
  → provenance, journal, explicit Worlds and named incomplete states

Compiler discipline
  → one validation pass, common data IR, target-specific packing

Hardware economy
  → packed BRAM stream, small FSM, generated masks, no cons expansion

Evidence discipline
  → host oracle, C artifact, RTL simulation, synthesis, board proof
```

> **UPC becomes powerful precisely because it does not try to replace phonology, Lisp, compilation or hardware. It gives each of them a small, stable representation boundary — and lets the whole ecosystem prove that boundary from several independent directions.**

## References

[1]: https://phoible.org/ "PHOIBLE 2.0 phonological inventory database"
[2]: https://phoible.org/faq "PHOIBLE FAQ: source variation and doculects"
[3]: https://wals.info/chapter/1 "WALS: Consonant Inventories"
[4]: https://wals.info/chapter/2 "WALS: Vowel Quality Inventories"
[5]: https://github.com/juv4uk/my-lisp/blob/main/docs/sanskrit-semantic-migration.md "my-lisp Sanskrit semantic migration: stable identity and representations"
[6]: https://github.com/juv4uk/my-lisp/blob/main/lib/world.my "my-lisp immutable World layer"
[7]: https://github.com/juv4uk/my-lisp/blob/main/lib/knowledge.my "my-lisp knowledge journal and data-only import"
[8]: https://github.com/juv4uk/my-lisp/blob/main/language-contract.my "my-lisp machine-readable language contract"
[9]: https://github.com/juv4uk/cml/blob/master/src/main.rs "CML compilation pipeline"
[10]: https://github.com/juv4uk/cml/blob/master/src/ir.rs "CML common intermediate representation"
[11]: https://github.com/juv4uk/fpga-lisp/blob/master/isa-contract.my "fpga-lisp ISA contract"
[12]: https://www.internationalphoneticassociation.org/content/ipa-chart "International Phonetic Association: IPA chart"
[13]: https://www.rfc-editor.org/rfc/rfc20.html "RFC 20: ASCII format for Network Interchange"
[14]: https://www.ibm.com/docs/ssw_aix_72/globalization/iso8859_family.html "IBM: ISO8859 family"
[15]: https://www.unicode.org/Public/MAPPINGS/VENDORS/MICSFT/WINDOWS/CP1252.TXT "Unicode mapping: CP1252"
[16]: https://datatracker.ietf.org/doc/html/rfc3629 "RFC 3629: UTF-8"
[17]: https://github.com/juv4uk/my-lisp/blob/main/crates/my-lisp/src/value.rs "my-lisp runtime Value model"
[18]: https://github.com/juv4uk/my-lisp/blob/main/crates/my-lisp/src/eval/special_forms/file_io.rs "my-lisp raw byte file primitives"
[19]: https://github.com/juv4uk/my-lisp/blob/main/crates/my-lisp/src/eval/special_forms/tcp.rs "my-lisp TCP UTF-8 boundary"
[20]: https://github.com/juv4uk/my-lisp/blob/main/lib/result-status.my "my-lisp tagged result-status convention"
[21]: https://github.com/juv4uk/cml/blob/master/src/compiler.rs "CML FPGA emitter and quoted-list construction"
[22]: https://github.com/juv4uk/cml/blob/master/tests/conformance_test.rs "CML blind FPGA conformance adapter"
[23]: https://github.com/juv4uk/fpga-lisp/blob/master/README.md "fpga-lisp measured FPGA resource budget"
[24]: https://github.com/juv4uk/fpga-lisp/blob/master/fpga/rtl/control.sv "fpga-lisp control FSM, byte I/O and monitor"
[25]: https://github.com/juv4uk/fpga-lisp/blob/master/docs/test-report-2026-08-17.md "fpga-lisp evidence levels and RTL simulation report"
