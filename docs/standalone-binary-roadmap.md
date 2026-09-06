# Standalone my-lisp binaries / Самостійні бінарники my-lisp

Status: execution roadmap. This document targets the **second variant**: not merely one demonstration executable, but a compiler path able to turn a broad, explicitly measured subset of ordinary my-lisp programs into standalone native binaries.

The focused machine-readable task queue is `standalone-binary-tasks.my`.

---

## Українська

### 1. Мета

Потрібно перейти від нинішнього доказу

```text
my-lisp -> CML IR -> C -> cc -> executable
```

до системи, де звичайна програма my-lisp у межах заявленого контракту компілюється у standalone binary без вбудованого Rust evaluator-а.

Ключовий критерій — не «файл запускається», а **семантична еквівалентність**:

```text
                 same source
                     |
        +------------+------------+
        |            |            |
        v            v            v
 native Rust     Lisp my-eval    compiled binary
 evaluator       meta-evaluator
        |            |            |
        +------------+------------+
                     |
              same observation
```

Для форм, які Lisp meta-evaluator ще не підтримує, тимчасовий мінімальний gate — native evaluator + compiled binary. Коли `my-eval` набуває parity, fixture переходить у тристоронній gate.

### 2. Що вже є

CML уже має спільний semantic IR, C backend, FPGA backend і вузький freestanding x86_64 backend. C backend уже компілював і реально запускав `def`, `lambda`, primitives, recursion, quoted lists, `let` і variadic parameter lists. Це означає, що standalone-напрям не починається з нуля.

Але поточна C runtime-підкладка ще не є повним runtime-контрактом my-lisp. Зокрема, не можна оголошувати general standalone parity, доки не закриті representation, errors, allocation/GC, strings, exact numbers, host capabilities і достатній conformance corpus.

### 3. Архітектурне правило

Не переносити весь Rust runtime у C рядок за рядком.

Розділяємо:

```text
LANGUAGE SEMANTICS                TARGET RUNTIME MECHANISM
-----------------                ------------------------
lexical rules                     allocation
macro meaning                     object tags
comparison policy                 raw bytes / syscalls
error identity                    process entry/exit
library semantics                 ABI calls
host-independent protocol         platform adapter
```

Те, що можна визначити у Lisp або у semantic IR один раз, не повинно дублюватися окремо у кожному backend.

### 4. Базова стратегія

C лишається першим portable native backend. LLVM не є передумовою.

```text
my-lisp source
      |
macro/language normalization
      |
semantic IR
      |
C backend
      |
small target runtime + generated C
      |
cc / clang / zig cc
      |
ELF / PE / Mach-O as supported by toolchain
```

Direct x86_64 backend лишається окремим freestanding напрямом і не повинен блокувати portable standalone binaries.

### 5. Рубежі

#### M0 — Gold triple oracle

Створити один harness, який бере один source fixture і порівнює:

1. native my-lisp evaluator;
2. Lisp `my-eval`, коли форма підтримана;
3. CML-generated executable.

Результат має бути структурованим: value/error/unsupported, а не grep тексту stdout.

#### M1 — Standalone driver

Одна команда повинна робити повний pipeline:

```text
cml build program.my -o program
```

Перший target — Linux x86_64 через системний C compiler. Driver повинен зберігати generated C за debug flag, показувати точну причину unsupported capability і не приховувати toolchain failure.

#### M2 — Runtime ABI v0

Винести з emitter-а явний runtime contract: Value representation, constructors, predicates, call convention, allocation ownership, error return channel, entry point.

Acceptance: generated program і runtime можуть збиратися окремими translation units без semantic залежності від приватних деталей emitter-а.

#### M3 — Core value completeness

Закрити типи, потрібні constitutive contract-у: empty list, symbols, proper/dotted pairs, strings, exact integers, exact rationals; inexact numbers лише після окремого рішення про representation.

Жодного silent truncation або implicit host conversion.

#### M4 — Closures and binding parity

Довести fixed, dotted, all-rest params; lexical capture; first-class builtins; self-recursion; потім mutual recursion. Arity mismatch має давати named, machine-readable failure.

#### M5 — Errors as data

Compiled executable повинен повертати стабільну error identity, яка порівнюється з language contract, а platform diagnostic зберігається лише як detail.

Не парсити тексти `gcc`, libc або OS для semantic decisions.

#### M6 — Memory discipline

Спочатку детермінований arena/region mode достатній для bounded fixtures. Потім — explicit GC milestone, коли живі програми доводять потребу.

Не вводити GC лише «бо Lisp повинен мати GC». Acceptance для GC: stress corpus, bounded memory growth, roots across globals/call frames/closures/current result.

#### M7 — Language libraries

Не компілювати кожну library primitive у C вручну. Завантажувані/статично включені `core.my`, macro layer і language-owned libraries мають проходити через той самий compiler pipeline, якщо їх semantics уже виразимі мовою.

#### M8 — Host substrate for executables

Standalone binary отримує малий capability ABI: files/process/time/TCP only when program requires them. Capability absence — explicit compile/link/runtime result, не hidden fallback.

Цей шар повинен узгоджуватися з `my-lisp/docs/host-portability-contract.md`.

#### M9 — Conformance ladder

Не чекати «100% мови» одним стрибком. Вести матрицю:

```text
fixture | native | my-eval | compiled | status | reason
```

Unsupported рахується окремо і ніколи не прирівнюється до pass.

Release gate для `standalone-v1`: усі constitutive fixtures, які не позначені контрактом як platform-only, або PASS, або мають явно ратифікований unsupported reason. Ціль — зменшити unsupported до нуля для core language.

#### M10 — Second host proof

Після Linux proof зібрати той самий corpus на другому host-і (Windows/MinGW або інший доступний C toolchain) без переписування language semantics.

Саме це доводить, що C backend є portable target, а не Linux-specific emitter.

### 6. Що не входить у v1

Не є блокерами першого general standalone release:

- LLVM backend;
- compiler, повністю переписаний на Lisp;
- direct machine-code JIT;
- повний freestanding OS boot;
- GPU/FPGA parity;
- оптимізуючий compiler;
- moving/generational GC без доказаної потреби.

Ці напрями можуть розвиватися паралельно, але не мають розмивати acceptance gate standalone binary.

### 7. Definition of done

`standalone-v1` можна назвати реальним лише коли один автоматичний corpus виконує:

```text
source
 -> normalize/lower
 -> C
 -> native binary
 -> execute
 -> decode observation
 -> compare with semantic oracle
```

і для заявленої поверхні немає silently skipped fixtures.

Сильне формулювання релізу:

> CML compiles the declared my-lisp standalone-v1 language surface to native executables, and every admitted conformance fixture is differentially checked against the reference semantics.

Не сильніше.

---

## English

### Goal

Move from a demonstrated `my-lisp -> CML IR -> C -> cc -> executable` path to a measured compiler path that can turn a broad declared my-lisp surface into standalone native programs without embedding the Rust evaluator.

The correctness criterion is differential semantics, not merely process exit success. The same source should be observed through the native evaluator, the Lisp metacircular evaluator where supported, and the compiled executable.

### Architecture

Keep C as the first portable native backend. LLVM is not required. Separate language meaning from target-runtime mechanism: semantic rules belong in Lisp/IR once; allocation, tags, ABI calls and raw OS effects belong in the small runtime adapter.

### Milestones

M0 builds the triple-oracle harness. M1 adds a one-command standalone build driver. M2 freezes a small runtime ABI v0. M3 completes core value representation. M4 closes closures/binding/recursion. M5 makes errors machine-readable. M6 adds memory discipline and only then GC if evidence requires it. M7 compiles language-owned libraries instead of reimplementing them in C. M8 maps the portable host substrate into standalone binaries. M9 drives all work from a no-silent-skip conformance matrix. M10 proves portability on a second host without rewriting semantics.

### Non-goals for v1

LLVM, a compiler rewritten entirely in Lisp, a JIT, full OS boot, GPU/FPGA parity, aggressive optimization and sophisticated GC are not prerequisites for the first general standalone release.

### Definition of done

A single automated corpus must perform source -> lower -> C -> native binary -> execute -> decode -> compare, with no silently skipped admitted fixtures. The release claim must name the exact supported language surface and remain no stronger than the evidence.
