# UPC на FPGA: де 8-бітне ядро реально економить ресурси, а де починає коштувати дорожче

**Автор:** Manus AI  
**Дата:** 18 серпня 2026 року  
**Статус:** архітектурна рекомендація для `shiva-sutras` / `fpga-lisp`; не RTL-зміна і не synthesis result

## Короткий висновок

Друже, з FPGA-погляду твоя ідея **8-bit core + рідкісні 2/3/4-byte extensions** дуже здорова. Але її головна цінність не в магічному числі 256. Цінність у тому, що frequent phonological data можна тримати **як packed byte stream поза cons heap**, читати sequentially, а metadata/profile/version винести у маленький control plane.

> Найдешевший sound на FPGA — не sound із «найкоротшою лінгвістичною назвою», а sound, який не перетворився на Lisp cons-cell, не потребує окремого 32-bit tagged word і не змушує evaluator виконати load/cons loop.

Для поточного `fpga-lisp` це ще важливіше, ніж для звичайної CPU-програми: heap складається з двох 32-bit BRAM arrays (`CAR` і `CDR`), має лише 4,096 physical cons cells, а ISA already заповнила всі 16 primary opcodes [1] [2] [3]. UPC therefore should start as a **data-plane buffer and decoder contract**, not as a new forest of Lisp objects or ad hoc ISA instructions.

## 1. Реальний економічний контекст твого FPGA

Поточна машина для Tang Primer 25K має 4,096-word instruction memory, 4,096-cons heap і 32-bit Lisp word: 4-bit tag plus 28-bit payload [1] [2]. Measured implementation на GW5A-25A already використовує 24 із 56 BSRAM blocks: 8 blocks для instruction memory та 16 blocks для heap. LUT/FF usage поки низький — approximately 1,363 LUTs and 952 FFs — але BRAM є більш відчутним budget [1].

| Поточний ресурс | Фактичний стан | Наслідок для UPC |
|---|---:|---|
| Lisp word | 32 bits | Зберігати один sound як boxed word уже у 4 рази дорожче за raw byte, ще до list links. |
| Cons cell | 64 bits (`CAR` + `CDR`) | List of byte values коштує 8 разів більше за packed bytes. |
| Heap | 4,096 cons cells | Corpus із 4,096 sound units повністю з’їсть heap, залишивши нічого evaluator-у. |
| BSRAM | 24/56 blocks used | Є місце для dedicated UPC buffer, але його треба вимірювати як окрему архітектурну ціну. |
| ISA | 16/16 opcodes occupied | UPC не варто починати з нового primary opcode. |

Якщо 4,096 UPC units зберігати як ordinary Lisp list of boxed fixnums, це потребує 4,096 cons cells × 64 bits = **262,144 bits**. Той самий raw byte stream потребує 4,096 × 8 = **32,768 bits**. Тобто packed representation is exactly **8× smaller** at the structural-storage level.

| Представлення 4,096 sound units | Пам’ять | Порівняння |
|---|---:|---:|
| Lisp list of fixnums | 262,144 bits | 8× baseline cost |
| Packed UPC bytes | 32,768 bits | 1× |
| 4 KiB dedicated packed store | 2 × 16-Kbit BSRAM blocks | Додає лише 2 BSRAM blocks |

На поточній платі additional 4 KiB raw UPC buffer theoretically moves BSRAM utilisation from **24/56 = 42.85%** to **26/56 = 46.42%**. Це не synthesis prediction для конкретного RTL: routing, width configuration і vendor inference треба реально перевірити. Але order of magnitude очевидний: two small BRAM blocks preserve entire Lisp heap instead of consuming it through cons cells.

## 2. Найбільша оптимізація: не будувати список узагалі

Якщо sequence of phonemes materialize-иться як Lisp list, hardware платить тричі:

1. data існує як 32-bit `FIXNUM` instead of 8-bit unit;
2. кожен element потребує `CONS` cell із 32-bit CAR and 32-bit CDR;
3. traversal uses heap access handshake, а не simple byte lane selection.

`lisp_data_unit.sv` intentionally gives `CAR`/`CDR`/`FETCH_PAIR` correct synchronous-BRAM behavior: reading a pair consumes an address phase and wait/data phase, and `FETCH_PAIR` avoids two separate traversals when both halves are needed [3]. Це хороша Lisp architecture — але sequence codec не повинен примушувати sound stream проходити саме цим шляхом.

Правильна physical model для UPC така:

```text
Lisp heap                       UPC data plane
─────────                       ──────────────
closures / env / rules          packed byte BRAM
proof terms / Worlds            sequence descriptor table
ordinary symbolic lists         profile/assignment manifest ROM
                                optional boundary sidecar
```

Lisp layer може посилатися на sequence через descriptor: `bank`, `offset`, `decoded-unit-count` або `byte-length`, `profile id`, `assignment version`. Але sound bytes themselves must remain packed.

## 3. 8-bit core як hardware fast path

У recommended UPC v1 grammar direct values occupy `00–DF`, while `E0`, `E1`, `E2` introduce deterministic 2-, 3-, 4-byte extension units. This preserves the current `00–4F` assignments and leaves direct-room without consuming a separate control range.

```text
00–DF          direct UPC unit
E0 xx          2-byte extension
E1 xx yy       3-byte extension
E2 xx yy zz    4-byte extension
E3–FF          reserved/invalid in UPC v1
```

Hardware consequence is favorable. The direct case requires only range comparison and byte emit; only rare lead bytes enter a small tail-count FSM.

```text
FETCH byte → classify first byte
                  │
     ┌────────────┼───────────────┐
     ▼            ▼               ▼
  00–DF         E0              E1 / E2
  emit          capture 1 tail   capture 2 / 3 tails
  direct        emit EXT2        emit EXT3 / EXT4
```

For a direct core unit, the datapath does not need a general variable-length parser, a division unit, a Huffman tree, or a dynamic dictionary. It needs a byte buffer, a few compares, a `tail_count` register and strict end-of-buffer validation. This is exactly the kind of predictable logic FPGA handles cheaply.

| Design choice | Hardware result |
|---|---|
| One-byte common units | Straight streaming fast path. |
| Lead byte determines exact unit length | No speculative parsing and no delimiter scanning. |
| Extension data is rare | Multi-cycle/tail state does not dominate common workloads. |
| Versions/profile at bank/frame level | No repeated metadata fetch per sound. |
| Strict malformed-sequence error | Corruption does not turn into a valid but wrong phonological unit. |

The one caveat is important. This `E0/E1/E2 + arbitrary tail` grammar is **deterministic when parsing starts at a known frame boundary**, but it is not fully UTF-8-like self-synchronising from an arbitrary corrupted byte offset, because a tail byte can have the same numeric value as a direct unit. For an FPGA streaming engine that always begins at `(base, byte_length)`, this is normally the right compromise. Do not sacrifice a large direct-code range merely to recover from random mid-stream entry unless a measured use case needs it.

If error recovery later matters, introduce block-level checksums and a small index of known frame starts. That gives bounded recovery without making every single sound unit more expensive.

## 4. Byte packing, word width і BRAM access

FPGA RAM is happier with 16/32-bit data ports than with thousands of separate tiny registers. One natural organization is a 32-bit BRAM word containing four UPC bytes.

```text
word[k] = { byte[4k+3], byte[4k+2], byte[4k+1], byte[4k+0] }

byte address a:
  word address = a >> 2
  lane         = a[1:0]
```

The decoder keeps a small register holding the current fetched word and a lane selector. At a direct unit it advances one lane; at an extension it consumes the declared number of lanes, fetching the next word only when needed. This avoids storing every byte as a Lisp word and avoids driving a giant mux across the entire corpus.

The byte order must be specified separately from `fpga-lisp` instruction-image endian. The ISA contract currently declares instruction words and UART boot length as little-endian [2]. That does **not** automatically decide the byte order within a UPC BRAM word. UPC needs its own explicit rule: for example, logical byte order increases with byte address and lane `0` is the least-significant eight bits. Once chosen, encode it into UPC test vectors and CML lowering tests.

## 5. Profiles, versions і boundaries мають бути control plane, не payload tax

`format_version`, `assignment_version`, `profile_id`, `analysis_level` and optional source/provenance are semantically necessary, but they do not belong before every phoneme. Put them in a **bank manifest** or sequence descriptor, not inline per unit.

```text
UPC bank manifest
  format version
  assignment-table version
  profile id + revision
  byte-order rule
  base address / byte length
  optional integrity/checksum

sequence descriptor
  offset · byte length · decoded-unit count · flags
```

For `fpga-lisp`, a compact descriptor can naturally fit in one or a few 32-bit words, unlike a linked list of thousands of cells. Whether offset/length/profile live in a separate BRAM or in an image section is a later implementation decision; the key is that they are **out of the hot byte loop**.

The same applies to linguistic boundaries. Storing a `WORD` or `MORPHEME` token inline after many sound units damages the ideal one-byte fast path and complicates decoder timing. When a workload needs boundary-aware rules, a sidecar indexed by decoded-unit position is often cheaper.

| Boundary design for 4,096 decoded units | Storage cost | FPGA implication |
|---|---:|---|
| Inline variable tokens | Depends on corpus and forces parser branches | Simple file syntax, poorer streaming locality. |
| One boundary bit per unit gap | 4,096 bits = 512 bytes | Cheap for a single boundary predicate. |
| 2-bit boundary kind per unit gap | 8,192 bits = 1,024 bytes | Encodes several mutually exclusive kinds. |
| Four independent bitplanes | 16,384 bits = 2,048 bytes | Supports overlapping annotations. |

This is an optimization only when those annotations are actually queried. For your current philosophy, `boundaries = absent` must remain a valid descriptor; do not pay sidecar BRAM for inventory experiments that only count or compare segments.

## 6. Natural classes: bitsets beat lists in the datapath

Phonological work often asks predicates like “is vowel?”, “is stop?”, “is dental?”, “belongs to pratyāhāra X?”. For direct one-byte UPC codes, a natural class can be implemented as a 256-bit membership mask: index by direct code, read one bit.

| Hardware representation | Capacity | Storage |
|---|---:|---:|
| One direct-code class mask | 256 code points | 256 bits = 32 bytes |
| Sixteen direct-code class masks | 16 predicates | 4,096 bits = 512 bytes |
| Direct-code feature ROM | 256 entries × 16 feature bits | 4,096 bits |
| Direct-code feature ROM | 256 entries × 32 feature bits | 8,192 bits |

This is much cheaper and more regular than dereferencing linked lists of phoneme symbols for every predicate. An extension can take a slower path: decode its full ID and consult an extension feature table only when such a unit appears. That gives precisely the behavior you want: **common sound classes are hardware-cheap; rare sounds remain representable without bloating the base datapath.**

Do not hard-code mutable linguistic claims into LUT equations. Generate class masks/feature ROM contents from the same versioned UPC registry that software uses. The hardware receives a reproducible table image; the registry, sources and epistemic status remain in the higher-level knowledge/provenance layer.

## 7. Чого не треба робити в RTL зараз

### Не додавати primary opcode

ISA contract 1.0 has all 16 opcodes allocated [2]. Existing architecture already handles extensions through documented modes and uses the post-`HALT` monitor protocol for extra observability without expanding ISA [4]. UPC should follow the same restraint.

If later a true accelerator is justified, expose it through an existing peripheral/I/O boundary or a contract-reviewed primitive path. `IN` and `OUT` already exchange single bytes: `IN` produces a fixnum byte and `OUT` emits low eight payload bits [4]. A future UPC stream engine can be made an explicit adjacent unit rather than overloading a random instruction field whose semantics become impossible to audit.

### Не робити `UPCSequence` як 4,096 cons cells

The current heap is designed for Lisp values, environments and continuations. It has a valid 2-cycle synchronous read discipline and a finite bump allocator; it is not an ideal byte-store [3]. A packed buffer must be separate from the list heap even if a Lisp-visible descriptor eventually refers to it.

### Не поспішати з entropy compression

Huffman, arithmetic coding, universal variable-bit codes, general dictionary compression and RLE may reduce archived corpus size, but they cost variable-bit alignment, more state, harder random access, additional error recovery rules and more verification surface. They are not the first optimization.

The first compression is structural: **one base phonological unit = one byte, packed four-per-32-bit word, with shared metadata per sequence.** If future corpus measurements show that extensions are frequent or UART transfer is the actual bottleneck, an external archive codec can decompress into canonical UPC bytes before FPGA inference. Keep the in-FPGA working representation simple.

## 8. UART і image economics

Current CML E2E harness sends program length as two little-endian bytes and then shifts every program byte across simulated UART. Its own comment records that an approximately 200-instruction, 800-plus-byte binary needs about 69.6 million simulation time units merely for UART loading; transport, not core execution, dominated that test [5].

This produces two distinct conclusions.

| Situation | What compact UPC helps | What it does not help |
|---|---|---|
| UPC corpus embedded naively as Lisp construction code | Shrinks code image, UART upload and heap allocation dramatically if moved to packed data. | It cannot make generic evaluator instruction stream shorter by itself. |
| External UPC bank loaded as bytes | Directly reduces load time proportional to byte count. | Requires a separate data load/frame path, not only the existing program bootloader. |
| Phonological compute on resident buffer | Reduces BRAM footprint and pointer chasing. | Does not automatically improve a rule whose algorithm is inherently expensive. |
| Offline archival | May benefit from additional compression. | Bit-level compressed format should not be forced into hot FPGA path prematurely. |

So UPC matters for speed **only if it replaces a larger representation that would otherwise move through UART, instruction memory or heap**. Do not claim an automatic speedup for all operations. Measure bytes transmitted, cycles per decoded unit and memory transactions for the specific workload.

## 9. Descriptor versus new Lisp tag

`fpga-lisp` currently declares six value tags (`FIXNUM`, `CONS`, `SYMBOL`, `NIL`, `TRUE`, `PRIMITIVE`) and therefore has nominal tag headroom [6]. However the broader `my-lisp` NaN-boxing plan already assigns additional tags for strings, rationals, closures and TCP handles. Thus it would be a mistake to casually reserve “the next free FPGA tag” for UPC without cross-repo agreement.

The cheapest staged path is:

1. **P0:** no first-class UPC value tag. Use a descriptor held in ordinary fixnums/structured data and a separate packed RAM bank.
2. **P1:** if compiled code needs first-class sequence values, introduce an opaque `UPC_BUFFER` or general `BYTES` descriptor only through a jointly versioned my-lisp/CML/fpga-lisp memory-layout contract.
3. **P2:** decide whether it is a pinned memory object, copied value, capability-like handle or GC-traced heap object only after root/GC semantics are explicit.

This avoids creating an elegant FPGA-only tag that later conflicts with canonical host representation.

## 10. Латентність і декодер: чесний contract

You do not yet have a measured UPC RTL path, so exact LUT/FF count, Fmax and cycles-per-unit would be invented if stated now. The right claim is a design target and a measurement plan.

| Metric | What to measure in the first RTL experiment |
|---|---|
| Storage | Added BSRAM blocks for byte bank, descriptors, feature masks, boundaries. |
| Logic | LUT/FF delta for byte fetch, lane select, prefix FSM and error state. |
| Timing | Worst slack/Fmax after place-and-route on GW5A-25A. |
| Throughput | Direct units per cycle and average units per cycle on real profile corpora. |
| Extension cost | Cycles and stalls for 2/3/4-byte units separately. |
| Rule cost | Feature-mask lookup versus existing list/heap traversal. |
| Transport | Bytes sent and UART load time for the same corpus in list/image versus packed UPC form. |
| Correctness | Exhaustive decoder grammar, malformed tails, profile/version mismatch, BRAM boundary crossing and block-integrity behavior. |

Your project already exposes performance counters for cycles, `CONS`, CAR/CDR accesses, evaluator calls, jumps and heap peak through the monitor protocol [4]. A UPC experiment should extend the evidence, not replace it: either add separately named UPC counters through the monitor protocol or collect them in a dedicated testbench. Do not hide data-plane cost inside a generic `cycles` total.

## 11. Рекомендований порядок робіт

| Пріоритет | Робота | Чому це перший крок |
|---|---|---|
| **P0** | Зафіксувати UPC v1 binary grammar: direct/lead ranges, byte order, frame/descriptor fields, strict malformed policy. | Hardware cannot optimize a changing or ambiguous grammar. |
| **P0** | Зробити host-side reference decoder and corpus profiler. | Потрібно виміряти direct/extension ratio before designing RTL. |
| **P0** | Створити `upc_bank` RTL experiment: packed BRAM + descriptor + 1/2/3/4-byte streaming validator, without ISA change. | Дає реальні LUT/BRAM/timing numbers with minimal semantic risk. |
| **P0** | Add exhaustive SystemVerilog and host-oracle test vectors. | Same grammar must be proven in prototype, CML tooling and RTL. |
| **P1** | Add direct-code natural-class masks/feature ROM generated from versioned registry. | Cheap hardware win for genuine phonological predicates. |
| **P1** | Add optional sidecar boundaries only for workloads that query them. | Preserves compact base corpus. |
| **P1** | Define data-loader or I/O peripheral protocol for UPC banks. | Avoids embedding large data as Lisp construction code. |
| **P2** | Consider first-class `Bytes`/UPC buffer tag after shared ABI decision. | Do not diverge from my-lisp/CML layout contract. |
| **P2** | Consider archive compression only after measured transport/storage bottleneck. | Avoids costly variable-bit hardware without evidence. |

## Висновок

Твій 8-bit UPC is not merely compatible with FPGA. It is one of the few representation choices that lines up naturally with what FPGA is good at: byte-addressed streaming, small deterministic state machines, ROM lookups, BRAM locality and fixed contracts.

The crucial discipline is to keep the layers separate:

```text
phonological meaning / profiles / provenance  → software + registry + Worlds
canonical UPC bytes                           → packed BRAM data plane
direct-unit feature predicates                → compact generated ROM/masks
rare extensions                               → slow but deterministic FSM path
transport/integrity                           → bank/frame layer
```

So the best first hardware goal is not “put all world phonetics into the FPGA”. It is:

> **Prove that a versioned UPC bank can be stored compactly, decoded strictly and queried by frequent natural classes without consuming cons heap, new primary opcodes or unmeasured complexity.**

If that experiment confirms the predicted 8× structural-memory advantage and acceptable timing, then the extension system gives you room to grow indefinitely without redesigning the common-case datapath.

## References

[1]: https://github.com/juv4uk/fpga-lisp/blob/master/README.md "fpga-lisp: measured GW5A-25A resources, 32-bit values and hardware budget"
[2]: https://github.com/juv4uk/fpga-lisp/blob/master/isa-contract.my "fpga-lisp ISA contract 1.0"
[3]: https://github.com/juv4uk/fpga-lisp/blob/master/fpga/rtl/lisp_data_unit.sv "Lisp data unit: cons heap and synchronous read FSM"
[4]: https://github.com/juv4uk/fpga-lisp/blob/master/fpga/rtl/control.sv "Control FSM, byte I/O, monitor protocol and performance counters"
[5]: https://github.com/juv4uk/fpga-lisp/blob/master/fpga/sim/tb_cml_e2e.sv "CML E2E UART loading and watchdog rationale"
[6]: https://github.com/juv4uk/fpga-lisp/blob/master/fpga/rtl/lisp_word.sv "Current fpga-lisp tagged word definition"
