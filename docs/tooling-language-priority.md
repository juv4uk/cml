# Tooling language priority: which cml modules should move to Lisp

Analysis written 2026-08-12, after `docs/heterogeneous-backends.md`'s
step 1/1.5 landed (`ir.rs`/`lower.rs`, `compiler.rs` consuming `Ir`) and
the question came up of whether `cml` itself — a compiler for a Lisp,
written in Rust — should follow `fpga-lisp`'s `assembler.py` →
`assembler.my` self-hosting move. Uses the same dividing line
`fpga-lisp/docs/tooling-language-priority.md` already established, applied
here to `cml`'s own modules instead of re-deriving it.

## Inventory

| Module | Lines | What it does | Data in / out |
|---|---|---|---|
| `parser.rs` | 108 | `.my` text → `ast::Expr` | text → Lisp data |
| `macros.rs` | 207 | `defmacro` expansion (compile-time only, never reaches fpga-lisp) | Lisp data → Lisp data |
| `lower.rs` | 170 | `ast::Expr` → `ir::Ir` | Lisp data → typed IR |
| `ir.rs` | 96 | IR type definitions | types only, no I/O |
| `compiler.rs` | 516 | `ir::Ir` → fpga-lisp ISA assembly text, register allocation | typed IR → hardware-adjacent bytes |

## The dividing line (restated from fpga-lisp)

**Migrate to Lisp where the tool transforms Lisp data. Don't, where
correctness depends on static types catching a class of bug that a
dynamically-typed reimplementation would have to re-earn by hand.**
fpga-lisp's version of this line was about *hardware I/O* (UART/serial);
`cml`'s version is about *register-allocation correctness* — this
session found three real bugs (`e73f93a`, `166dffa`, `2b66898`) exactly
where an un-typed, hand-maintained invariant (which registers survive a
nested call) was silently violated. That's the `cml`-specific reason to
keep the codegen layer in a statically-checked language, not a
transplanted rule.

## Per-module analysis

**`macros.rs` — highest priority, real self-hosting candidate.**
`defmacro` expansion is a tree-walking meta-evaluator over `quote`/
`cons`/`car`/`cdr`/`atom`/`eq`/`cond` (`compatibility.my`'s own
description) that never reaches fpga-lisp at all (`never-reaches-fpga .
true`). It is, structurally, a small Lisp interpreter written in Rust —
the exact shape `assembler.py`→`assembler.my` already proved worth
porting, and arguably a stronger case: a macro-expander written *in* the
language whose macros it expands is the self-hosting move, not just a
tool that happens to consume Lisp-shaped data.

**`lower.rs` — Lisp-shaped, but coupled to `ir.rs`'s Rust types.**
`ast::Expr -> Ir` is a pure data transformation and reads as naturally
in Lisp as `macros.rs` does. Lower priority than `macros.rs` in
practice: its output (`Ir`) is consumed directly by `compiler.rs` as
native Rust types, so porting `lower.rs` alone would need a
serialization boundary back into Rust that doesn't exist today — a real
project, not a small one, and not obviously worth it unless `ir.rs`
moves too (see below, it shouldn't).

**`parser.rs` — Lisp-shaped in principle, but the wrong tool for this
repo's deployment model.** A `.my` reader is exactly what `my-lisp`
itself already implements authoritatively (and exposes live via the TCP
oracle's `parse` op). But `cml` is a standalone AOT CLI/library with no
runtime `my-lisp` dependency — that's load-bearing for its CI story (no
live process needed to compile) and its use as a library. Delegating
parsing to `my-lisp` would mean either a live process dependency
(breaks the standalone-binary story) or embedding `my-lisp` itself as a
dependency (which is Rust anyway — porting `cml`'s reader to `my-lisp`
source wouldn't even remove a Rust dependency, just move which Rust
parser is in the loop). Not worth pursuing.

**`ir.rs`/`compiler.rs` — not migration candidates, by design.** These
are `cml`'s version of fpga-lisp's "operates hardware" exclusion: `ir.rs`
is the typed contract every current and future backend shares (its
whole reason to exist per `docs/heterogeneous-backends.md` is to be a
stable, checkable interface — untyped data would defeat that purpose),
and `compiler.rs` is the register-allocation layer where `docs/abi.md`'s
"one rule that matters" lives. Rust's exhaustive `match` over `Ir`'s
variants is doing real correctness work here (a missing arm is a
compile error, not a silent miscompile) — the same category of
protection static types gave `lower.rs`'s rewrite this session (catching
the string-literal lowering bug at the type level, not by re-deriving
register discipline in an untyped host).

## Priority summary

1. **`macros.rs` → a `.my`-hosted macro-expander**: ✅ done (`macros.my`,
   this repo root). Differentially verified against two representative
   fixtures via the real `my-lisp` CLI (see `macros.my`'s own status
   note for both, and the two real bugs found+fixed while doing it).
   Not wired into `cml`'s actual pipeline -- same status as `fpga-lisp`'s
   `assembler.my` relative to `assembler.py`, a proven parallel
   implementation, not the thing CI/evidence exercise.
2. **`lower.rs` → Lisp**: plausible later, not now. Only makes sense
   paired with a real Rust↔Lisp data boundary for `Ir`, which doesn't
   exist and isn't otherwise needed yet.
3. **`parser.rs` → Lisp**: not planned. `my-lisp`'s own reader is
   already the authoritative one; `cml` reimplementing it in Rust is
   about deployment independence (no live process/embedded interpreter
   dependency), not a language-suitability question.
4. **`ir.rs`/`compiler.rs` → Lisp**: not planned, not desired. Same
   reasoning fpga-lisp's `upload.py`/`monitor.py` verdict used, applied
   to a different kind of "hardware": if a future agent proposes this,
   point here first rather than re-deriving why it's a bad trade.

---

# Пріоритети мов інструментарію: які модулі cml мають мігрувати на Lisp (Ukrainian)

Аналіз написано 2026-08-12, після завершення кроку 1/1.5 із `docs/heterogeneous-backends.md` 
(`ir.rs`/`lower.rs`, `compiler.rs` споживає `Ir`), коли постало питання, чи повинен сам `cml` 
(компілятор для Lisp, написаний на Rust) наслідувати приклад `fpga-lisp` із його 
переходом `assembler.py` → `assembler.my` до самохостингу. Використовує ту саму межу, 
яку вже встановив документ `fpga-lisp/docs/tooling-language-priority.md`, застосовуючи її 
до власних модулів `cml`, а не виводячи заново.

## Інвентаризація

| Модуль | Рядки | Що робить | Дані вхід / вихід |
|---|---|---|---|
| `parser.rs` | 108 | текст `.my` → `ast::Expr` | текст → Lisp-дані |
| `macros.rs` | 207 | розширення `defmacro` (тільки compile-time, ніколи не досягає fpga-lisp) | Lisp-дані → Lisp-дані |
| `lower.rs` | 170 | `ast::Expr` → `ir::Ir` | Lisp-дані → типізований IR |
| `ir.rs` | 96 | визначення типів IR | тільки типи, без I/O |
| `compiler.rs` | 516 | `ir::Ir` → асемблер fpga-lisp ISA, розподіл регістрів | типізований IR → байти, близькі до апаратних |

## Розділова лінія (повторення з fpga-lisp)

**Переходьте на Lisp там, де інструмент перетворює Lisp-дані. Не робіть цього там, 
де коректність залежить від того, що статичні типи ловлять певний клас помилок, 
який у динамічно-типізованій реалізації довелося б заново перевіряти вручну.** 
Для `fpga-lisp` ця лінія стосувалася *апаратного вводу/виводу* (UART/serial); 
версія `cml` стосується *коректності розподілу регістрів* — під час сесії 
було знайдено три реальні помилки (`e73f93a`, `166dffa`, `2b66898`) саме там, 
де нетипізований, підтримуваний вручну інваріант (які регістри виживають після 
вкладеного виклику) непомітно порушувався. Це специфічна для `cml` причина тримати 
рівень кодогенерації у статично перевіряємій мові, а не просто перенесене правило.

## Помодульний аналіз

**`macros.rs` — найвищий пріоритет, справжній кандидат на самохостинг.**
Розширення `defmacro` — це мета-обчислювач, що обходить дерево поверх `quote`/
`cons`/`car`/`cdr`/`atom`/`eq`/`cond` (як описано у `compatibility.my`), який ніколи 
не досягає fpga-lisp (`never-reaches-fpga . true`). Структурно це невеликий 
Lisp-інтерпретатор, написаний на Rust — точно така форма, яку `assembler.py`→`assembler.my` 
вже довели як варту портування, і, ймовірно, це ще вагоміший випадок: розширювач 
макросів, написаний *тією самою мовою*, чиї макроси він розширює — це крок до 
самохостингу, а не просто інструмент, що випадково споживає Lisp-подібні дані.

**`lower.rs` — Lisp-подібний, але прив'язаний до Rust-типів `ir.rs`.**
`ast::Expr -> Ir` — це чисте перетворення даних, яке так само природно читається 
на Lisp, як і `macros.rs`. Проте на практиці пріоритет нижчий за `macros.rs`: його 
вивід (`Ir`) споживається безпосередньо модулем `compiler.rs` як нативні Rust-типи, 
тому портування лише `lower.rs` вимагало б межі серіалізації назад у Rust, якої 
сьогодні не існує. Це великий проєкт, який неочевидно, що вартий зусиль, якщо `ir.rs` 
також не мігрує (а він не повинен, див. нижче).

**`parser.rs` — Lisp-подібний у принципі, але невідповідний інструмент для моделі розгортання цього репозиторію.** 
Reader `.my` — це саме те, що `my-lisp` уже реалізує авторитетно (і експонує наживо 
через операцію `parse` TCP-оракула). Але `cml` — це окрема AOT CLI-утиліта/бібліотека 
без runtime-залежності від `my-lisp` — що є ключовим для CI (для компіляції не 
потрібен живий процес) і його використання як бібліотеки. Делегування парсингу до 
`my-lisp` означало б або залежність від живого процесу (що ламає ідею standalone бінарника), 
або вбудовування самого `my-lisp` як залежності (що все одно Rust — портування рідера `cml` 
до коду `my-lisp` навіть не усунуло б залежність від Rust, а лише перемістило б те, 
який Rust-парсер використовується). Не варто продовжувати роботу в цьому напрямку.

**`ir.rs`/`compiler.rs` — не кандидати на міграцію, за дизайном.** 
Це версія винятку "керує апаратним забезпеченням" з fpga-lisp, адаптована для `cml`: 
`ir.rs` є типізованим контрактом, який розділяють усі поточні та майбутні бекенди (весь 
сенс його існування, згідно з `docs/heterogeneous-backends.md`, — бути стабільним 
інтерфейсом, який можна перевірити; нетипізовані дані знищили б цю мету), а `compiler.rs` 
є рівнем розподілу регістрів, де живе "єдине правило, що має значення" з `docs/abi.md`. 
Вичерпний `match` у Rust по варіантах `Ir` виконує тут реальну роботу з контролю 
коректності (відсутня гілка — це помилка компіляції, а не непомітна помилка кодогенерації) — 
це та сама категорія захисту, яку статичні типи дали перезапису `lower.rs` під час цієї сесії.

## Підсумок пріоритетів

1. **`macros.rs` → макро-розширювач на `.my`**: ✅ виконано (`macros.my`, 
   корінь цього репо). Диференційно перевірено на двох репрезентативних 
   фікстурах через реальний CLI `my-lisp`. 
   Не інтегровано у фактичний пайплайн `cml` — такий самий статус, як у `assembler.my` 
   відносно `assembler.py` у `fpga-lisp`: доведена паралельна реалізація, 
   а не та річ, яку зараз тренують CI та докази (evidence).
2. **`lower.rs` → Lisp**: можливо пізніше, не зараз. Має сенс лише в парі з реальною 
   межею даних Rust↔Lisp для `Ir`, якої ще не існує і яка поки не потрібна.
3. **`parser.rs` → Lisp**: не планується. Рідер самого `my-lisp` уже є авторитетним; 
   переписування його в `cml` на Rust стосується незалежності розгортання (відсутність 
   залежності від живого процесу/вбудованого інтерпретатора), а не питання, чи підходить мова.
4. **`ir.rs`/`compiler.rs` → Lisp**: не планується, не бажано. Та сама аргументація, 
   що й для `upload.py`/`monitor.py` у fpga-lisp: якщо майбутній агент запропонує це, 
   вкажіть спочатку сюди, замість того, щоб наново виводити, чому це поганий обмін.
