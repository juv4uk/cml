# UPC з погляду `my-lisp`: що справді важливо для мови, reasoning і майбутнього hardware

**Автор:** Manus AI  
**Дата:** 18 серпня 2026 року  
**Статус:** архітектурна рекомендація; не зміна контракту `my-lisp`

## Головний висновок

Для абстрактного UPC ми щойно визначили: потрібні extension prefixes, versioning, frame boundaries, error contract і за потреби linguistic boundaries. Для `my-lisp` найважливіше інше: **не дати байтовому коду стати semantic identity.**

У тебе вже є правильний precedent. Sanskrit semantic layer прямо каже: `SLP1` є canonical source representation, IAST — лише presentation, а semantic ID не може дорівнювати spelling [1]. `SemanticCall` також працює зі stable atom IDs, не з raw SLP1 strings [2]. UPC має увійти в систему за тим самим принципом.

> **UPC byte — це representation. Segment identity — це stable registry entity. Фонологічний факт — це твердження про occurrence у конкретній послідовності, profile, world і джерелі.**

Якщо тримати ці три речі окремо, UPC стане сильним bridge між `shiva-sutras`, `my-lisp-panini`, `my-lisp`, CML і FPGA. Якщо змішати їх, зміна mapping або profile може почати змінювати логічний зміст already-stored facts.

## 1. Де UPC має жити в архітектурі

`my-lisp` уже розділяє source/presentation/semantic identity. Цю ж схему варто поширити на звукові дані.

```text
Input / display layer
  SLP1 · IAST · Devanāgarī · Ukrainian orthography · IPA
                        │  language/profile-specific codec
                        ▼
Phonological IR
  stable SegmentId + ordered occurrences + modifiers + boundaries
                        │  canonical UPC serializer
                        ▼
UPC binary layer
  1-byte direct units · 2/3/4-byte extensions · framing metadata
                        │
                        ▼
Storage / transport / CML / FPGA
```

| Шар | Його відповідальність | Чого він не повинен робити |
|---|---|---|
| **Input/display** | Прийняти або показати `SLP1`, IAST, Devanāgarī, orthography, IPA. | Не визначати ultimate identity звука. |
| **Phonological IR** | Зберегти sequence of stable segments, boundaries, modifiers і profile context. | Не залежати від одного написання або конкретного byte layout. |
| **UPC binary** | Compact canonical serialization цього IR для storage, interchange, hardware. | Не бути knowledge fact або правилом reasoning сам по собі. |
| **Semantic/reasoning** | Оперувати predicates, features, rules, proofs і provenance. | Не виводити факти безпосередньо з hex values. |

Це не абстрактна чистота. Уже наявний `Semantic Atom Registry` перевіряє механічно, що `id` не дорівнює `slp1`; registry також зберігає category, aliases, status, semantic description та presentation forms [3]. Для phonology потрібний такий самий registry discipline, але з `SegmentId` замість dhātu/kāraka atom.

## 2. Найважливіше розділення: code point, segment і occurrence

У UPC не треба називати один byte «фонемою» без уточнення. Для `my-lisp` є три різні сутності.

| Сутність | Приклад | Стабільність | Для чого потрібна |
|---|---|---|---|
| **UPC code unit** | `0x17` або sequence `E1 02 7F` | Stable лише у конкретній `upc-format` + `assignment-version`. | Binary storage, wire format, hardware lookup. |
| **SegmentId** | `SEG_…` із registry | Stable semantic/phonological identity. | Features, natural classes, cross-profile rules. |
| **Occurrence** | «третій unit у sequence `S`, перед morpheme boundary» | Залежить від конкретного input/world. | Sandhi, derivation, proof, edit, provenance. |

Приклад correctly separated data може виглядати концептуально так:

```lisp
; Registry fact — не rule і не occurrence
(segment SEG_0042
  (upc-code #x42)
  (inventory upc-base-v1)
  (features ((manner . stop) (place . dental) (voice . voiceless)))
  (status stable))

; Sequence — конкретний фонологічний об’єкт
(phon-sequence SEQ_381
  (profile sanskrit-phonemic-v1)
  (analysis-level phonemic)
  (segments (SEG_0001 SEG_0042 SEG_0017))
  (boundaries ((after 1 morpheme)))
  (source SOURCE_17))
```

Нотація `#x42` тут лише пояснювальна; поточний reader `my-lisp` не обов’язково вже має такий literal. Суть у тому, що **`SEG_0042` не повинен бути самим числом `66`, а byte `0x42` не повинен бути єдиним джерелом змісту сегмента**.

Це особливо важливо для твоїх language profiles. Одна і та сама speech-unit може бути спільною між Sanskrit, Ukrainian, Polish або English profiles; різниться mapping from source form to segment sequence, а не identity звука. І навпаки, одна графема в різних profiles може давати різні sequences.

## 3. Те, що `my-lisp` може зробити вже зараз

У current runtime немає `ByteVector`: `Value` має `String(Rc<str>)`, symbols, numbers, pairs, closures/macros і TCP handles, але не raw byte array [4]. Це не проблема для першого UPC layer.

`read-file-bytes` уже повертає raw file bytes як proper list of exact integers `0–255`, а `write-file-bytes` перевіряє та записує такий список без UTF-8 transcoding [5]. Отже, для **P0 prototype** можна подати UPC payload саме як validated proper list of bytes.

```lisp
; P0: data representation, не новий built-in type
((kind . upc-sequence)
 (format . upc-8)
 (format-version . (1 0))
 (assignment-version . (1 0))
 (profile . sanskrit-phonemic)
 (profile-version . (1 0))
 (analysis-level . phonemic)
 (bytes . (0 23 66 17))
 (boundaries . ())
 (source . source-17))
```

Тут функція `upc-sequence-valid?` має перевірити не тільки `0 ≤ n ≤ 255`, а й усю grammar: direct byte, exact 2/3/4-byte extension length, forbidden prefix, unknown assignment або truncated tail. Не можна обмежитися тим, що `write-file-bytes` already validates numeric range: `E1 02` може бути valid byte list, але malformed UPC unit.

**Чому не `String`?** `Value::String` є `Rc<str>`, а TCP layer читає bytes як UTF-8 і explicitly rejects non-UTF-8 data; `tcp-write` likewise writes UTF-8 bytes from a string [4] [6]. UPC payload може містити `0x00`, `0x80`, invalid UTF-8 prefixes або будь-яке інше byte value, тож він не є `my-lisp` text. Не слід перетворювати payload на string і не слід посилати його через current `tcp-write` as if it were text.

## 4. Що `my-lisp` мусить отримати до реального binary UPC usage

Коли UPC перестане бути навчальним/list-based prototype і стане регулярним storage, compilation або hardware path, потрібен окремий immutable `ByteVector`/`Bytes` value. Це буде не «ще один зручний primitive», а тип, що бере на себе інваріант raw binary data.

| Властивість майбутнього `Bytes` | Чому вона потрібна UPC |
|---|---|
| Immutable `Rc<[u8]>` або еквівалент | Добре узгоджується з persistent data model, structural sharing і World snapshots. |
| Structural equality by bytes | Два однакові serialized UPC payloads мають бути рівними як data. |
| Distinct from `String` | Гарантує, що Unicode/UTF-8/text display не змінять binary payload. |
| Length known in value | `0x00` не стає terminator/padding і не втрачається. |
| Canonical readable rendering | Наприклад, `#u8(0 23 66 17)` або hex notation, що round-trips through `read`/`write`. |
| Bounds-safe access and slicing | Потрібні parser-у, extension decoder-у й CML lowerer-у. |
| Explicit conversion functions | `bytes↔list`, `bytes↔hex`, `bytes↔Base64`; жодної silent coercion до `String`. |

Тут є один важливий ecosystem constraint. `my-lisp` має NaN-boxing layout, що узгоджує lower 32 bits із 4-bit tags FPGA ISA; actual runtime already uses tags `0–11` for values на кшталт fixnum, cons, string, rational, closure, macro й TCP handles [7]. Тобто `Bytes` не можна додати «лише в Rust enum»: якщо він має бути part of compiled/FPGA-visible data model, потрібно узгодити tag allocation, heap representation і conformance між `my-lisp`, CML і `fpga-lisp`.

Це не означає робити його зараз. Навпаки, твій принцип **додавати можливість тоді, коли вона реально потрібна** тут ідеально підходить:

1. **Зараз:** `list of exact 0–255` + UPC validator/library + binary file I/O.
2. **Коли з’явиться hot path, великий corpus або binary TCP:** immutable `Bytes` for host runtime.
3. **Коли UPC стане ISA/storage concern:** спільний tag + heap object + CML/FPGA conformance fixtures.

## 5. UPC і knowledge/provenance: саме тут my-lisp додає найбільшу цінність

`knowledge.my` уже робить важливе для UPC розділення: external knowledge входить через guarded `advise`/`advise-all` як data, валідується, перевіряється на explicit conflict і лише тоді потрапляє в append-only journal [8]. `World` зберігає explicit journal and metadata; reasoning adapters залежать від переданого world, а не від hidden global state [9].

Для UPC це означає: **mapping table, language profile і phonological assertion не мають бути просто файлом, який “зараз лежить у репозиторії”.** Вони мають бути versioned knowledge with provenance.

```lisp
; Схема, не фінальний API
(upc-profile sanskrit-phonemic-v1
  (upc-format-version (1 0))
  (assignment-version (1 0))
  (normalization slp1-canonical-v1)
  (source source-shiva-sutras)
  (status experimental))

(profile-maps sanskrit-phonemic-v1 "K" (SEG_KH))
(segment-code upc-base-v1 SEG_KH (35))
```

Тоді proof може відповідати не лише «що це `SEG_KH`», а й:

```text
який profile застосовано;
яка версія assignment table;
який source/assumption дав mapping;
у якому World і яким rule це було виведено.
```

Це і є місце, де `my-lisp` набагато цікавіший за звичайний binary codec. У простій library ти одержиш `bytes → sequence`; у тебе можна отримати **`bytes → sequence → rules/features → proof → branchable world`**.

## 6. Що reasoning має бачити, а що ні

Reasoning engine не повинен працювати з byte arithmetic як із primary linguistic semantics.

| Завдання | Коректний рівень у my-lisp |
|---|---|
| «Цей сегмент — dental stop» | Registry feature fact for `SegmentId`. |
| «У цій послідовності segment X стоїть перед Y» | Sequence/occurrence relation. |
| «Застосувати sandhi rule на morpheme boundary» | Phonological IR + typed boundary + profile. |
| «Зберегти/надіслати/завантажити sequence» | UPC binary layer. |
| «Визначити, чи `0x31 < 0x42`» | Лише коли rule справді про binary format, never as a proxy for phonetics. |

Інакше виникне небезпечна прихована залежність: зміниш layout або введеш extension — і лінгвістичне rule перестане працювати, хоча phonological model не змінилася. `my-lisp` уже добре показує кращий pattern: semantic atom ID незалежний від SLP1 spelling [1] [3]. Для UPC потрібно сказати те саме ще раз: **phonological inference незалежний від byte spelling.**

## 7. У якому форматі UPC має подорожувати через існуючий agent/knowledge protocol

Current knowledge package deliberately serializes data as one re-readable S-expression and receives it only as data, not `eval` [8]. Це правильний шлях для small/medium sequence exchange та agent messages. У ньому raw bytes можна безпечно показати як validated list або explicit hex/Base64 string, але не як opaque `String` containing arbitrary bytes.

| Сценарій | Найкращий формат зараз | Причина |
|---|---|---|
| Knowledge package, git, Markdown, proof | S-expression envelope з `(bytes . (…))` або canonical hex | Readable, data-only, stable diff, no UTF-8 confusion. |
| Local binary corpus/file | `write-file-bytes` / `read-file-bytes` + length/profile manifest | Already supports raw bytes. |
| Current TCP knowledge exchange | Hex/Base64/text-safe encoding inside S-expression | Current TCP primitive є UTF-8 text-only. |
| Future high-throughput TCP | New binary capability over `Bytes` + UPC frame length + integrity | Не ламати існуючий text knowledge transport. |
| CML/FPGA pipeline | `Bytes` or framed memory object after shared ABI contract | Потрібні tag, heap, endian/length і conformance agreement. |

Тобто raw binary transport — **не upgrade existing text TCP in place**. Better: лишити knowledge package як trusted data envelope; коли виникне реальна потреба, додати separate capability named explicitly, наприклад `tcp-read-bytes` / `tcp-write-bytes`, із length/resource limits і без змішування з `read`/`eval`.

## 8. Error states повинні бути лінгвістично чесними

Твій `result-status.my` already separates `unknown`, `partial`, `blocked` і `disputed` instead of collapsing them into `()` [10]. UPC decoder/importer має продовжити цю традицію.

| Стан | Приклад | Правильний результат |
|---|---|---|
| `malformed` | `E1` без двох tail bytes | Structured decode error: offset, bytes, format version. |
| `unsupported` | Valid extension namespace, unknown to this implementation | `blocked` or tagged `unsupported-extension`, not a guessed phoneme. |
| `unknown-segment` | Profile says source token exists, but mapping absent | Explicit unknown phonological analysis with source context. |
| `ambiguous` | Two profiles legitimately map spelling differently | `disputed`/choice-required with candidate profiles. |
| `partial` | Corpus parser hit bounded resource limit | `partial`, not «no phonemes found». |
| `corrupted` | Integrity check or byte validation fails | Transport/data error; never substitute a regular segment silently. |

Особливо важливо не змішати `unknown-segment` з `malformed-bytes`. Перше — чесний факт про неповноту твоєї model; друге — failure representation/transport. Це різні події, отже вони мають різні provenance, remediation і consequences for reasoning.

## 9. Практичний мінімальний API

Не обов’язково додавати все це зараз, але namespace можна спроєктувати відразу, щоб не довелося ламати public API:

```lisp
; P0 library-level operations over validated byte lists
(upc-encode profile source-form)       ; -> (accepted upc-sequence ...)
(upc-decode upc-sequence)              ; -> (accepted phonological-ir ...)
(upc-validate upc-sequence)            ; -> accepted | rejected diagnostic
(upc-hex upc-sequence)                 ; human/text-safe representation
(upc-from-hex manifest text)           ; validates grammar + metadata
(upc-segments upc-sequence)            ; decode to SegmentId sequence

; Explicit reasoning boundary
(phon-features segment-id)
(phon-before? sequence occurrence-a occurrence-b)
(phon-boundary? sequence offset boundary-kind)
```

`upc-encode` має повертати structured status, а не `()` on failure. `upc-decode` має повертати IR tagged with profile/version, а не plain list of numbers stripped from context. А `phon-features` і phonological rules мають приймати `SegmentId`, never raw byte code.

## 10. Рекомендований порядок робіт

| Пріоритет | Робота | Виграш |
|---|---|---|
| **P0** | Створити `lib/upc.my` як pure library над list-of-bytes: validator, 1/2/3/4-byte decoder, codec profile envelope, hex renderer. | Дає реальний executable contract без зміни runtime/FPGA. |
| **P0** | Описати machine-readable `upc-contract.my` окремо від `language-contract.my`. | Правильно продовжує твою модель незалежних contract versions [11]. |
| **P0** | Ввести stable `SegmentId` registry + mapping `SegmentId ↔ UPC assignment ↔ profile representation`. | Не дозволяє bytes або SLP1 стати semantic identity. |
| **P0** | Додати exhaustive property tests: canonical decode/encode, malformed extension, unknown profile, version mismatch, `read-file-bytes` round-trip. | Захищає саме ті failure modes, що можуть silently poison reasoning. |
| **P1** | Зробити `Bytes` immutable value only when list overhead/transport реально заважають. | Binary correctness and performance без premature ISA cost. |
| **P1** | Зв’язати UPC manifests з World metadata/provenance. | Reproducible derivation навіть після еволюції profiles. |
| **P2** | Узгодити `Bytes` tag, heap contract і conformance across my-lisp/CML/fpga-lisp. | Справжній hardware path without ABI drift. |

## Висновок

Для твого Lisp UPC важливий не стільки як «256 кодів для звуків», скільки як тест на архітектурну дисципліну, яку ти вже вмієш будувати.

`my-lisp` already має потрібні ідеї: explicit contracts instead of commit folklore, semantic IDs independent from spelling, exact data, data-only knowledge ingestion, append-only journal, immutable Worlds і structured incomplete outcomes [1] [8] [9] [10] [11]. UPC має не обійти ці принципи, а отримати від них силу.

> **Найкращий UPC для my-lisp — це не нова “строка фонем”. Це versioned binary codec під phonological IR, який зберігає provenance, не плутає bytes із meaning, не змінює old facts після еволюції profiles і може бути lowered до hardware лише тоді, коли semantic contract already стабільний.**

## References

[1]: https://github.com/juv4uk/my-lisp/blob/main/docs/sanskrit-semantic-migration.md "Sanskrit semantic migration: canonical SLP1, presentation and stable identity"
[2]: https://github.com/juv4uk/my-lisp/blob/main/crates/my-lisp/src/semantic/karaka.rs "SemanticCall and validation through stable atom IDs"
[3]: https://github.com/juv4uk/my-lisp/blob/main/crates/my-lisp/src/semantic/atoms.rs "Semantic Atom Registry and identity/spelling invariants"
[4]: https://github.com/juv4uk/my-lisp/blob/main/crates/my-lisp/src/value.rs "Runtime Value model"
[5]: https://github.com/juv4uk/my-lisp/blob/main/crates/my-lisp/src/eval/special_forms/file_io.rs "Raw binary file primitives via validated byte lists"
[6]: https://github.com/juv4uk/my-lisp/blob/main/crates/my-lisp/src/eval/special_forms/tcp.rs "TCP primitives and UTF-8 text boundary"
[7]: https://github.com/juv4uk/my-lisp/blob/main/crates/my-lisp/src/layout.rs "NaN-boxing layout and FPGA-compatible tag space"
[8]: https://github.com/juv4uk/my-lisp/blob/main/lib/knowledge.my "Data-only knowledge package, advice and journal"
[9]: https://github.com/juv4uk/my-lisp/blob/main/lib/world.my "Immutable Worlds and explicit reasoning context"
[10]: https://github.com/juv4uk/my-lisp/blob/main/lib/result-status.my "Tagged states: unknown, partial, blocked, disputed"
[11]: https://github.com/juv4uk/my-lisp/blob/main/language-contract.my "Independent machine-readable compatibility contract"
