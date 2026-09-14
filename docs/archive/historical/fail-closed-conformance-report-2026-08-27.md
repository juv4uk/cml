# CML conformance — fail-closed classification report (2026-08-27)

**Task:** `CML-FULL-SEMANTIC-IR` (cml tasks.my, priority 7.5)
**Scope:** Phase 4 of `CML-CONTRACT-REALIGNMENT-M0` (Viveka's analysis, §9 Phases 1-4).
**Why this slice:** Phases 1-3 touch `my-lisp` semantics (contract 2.1, first-class
builtins) and are authority-layer work outside this repo. Phase 4 — *make conformance
fail closed* — is a reporting change local to `cml` and is the only part of the
milestone that does not require moving another repository's contract first. It is also
the prerequisite that makes the rest of the milestone checkable.

**Live evidence:** `cargo test --test conformance_test` — **2 passed, 0 failed, 101s**
(`tests/conformance_test.rs:203-413`, run from `cml/`).

---

## 1. What the runner does today (as-built, not aspirational)

`test_conformance` reads `../my-lisp/tests/fixtures/conformance.my`, runs each Tier-1
fixture through `parse → macro-expand → lower → compile → assemble (fpga-lisp
`assembler.py`) → simulate (`vvp tb_cml_e2e.sv`) → decode heap`, and asserts the
decoded value equals the fixture's `expected`. Error fixtures assert `RESULT_ERROR`.

Admission gates currently in the loop (`conformance_test.rs:260-273`):

| Gate | line | behavior |
|---|---|---|
| Tier filter | 261 | `if !line.contains("(tier . 1)") { continue; }` — **silently skips all Tier-2 and Tier-3 fixtures** |
| Contract gate | 265-268 | skips `since-contract > (2,0)`, counts them as `unsupported_newer_contract` |
| Float gate | 271-273 | skips any line containing `3.0` |

**Fixture inventory** (from `conformance.my`, 224 non-empty top-level forms):
- Tier 1: 35 — of which 32 actually exercised (3 skipped by the float gate)
- Tier 2: 105 — **zero exercised**
- Tier 3: 84 — **zero exercised**

## 2. The honest classification (Phase 4's required vocabulary)

Per the milestone, every fixture must land in exactly one of:

```
SUPPORTED          — executed, value matches expected
UNSUPPORTED-BY-DESIGN  — excluded by a named, versioned hardware limit
UNIMPLEMENTED      — in scope for the backend, not yet lowered/compiled
UNVERIFIED         — ran but produced no decodable result
FAILED             — executed and disagreed with expected
```

The current runner conflates the first three into a single bare `continue` with no
record. A future reader (or a second backend) cannot tell "CML does not support Tier 2"
from "the test is just not wired up to Tier 2" — and the second is the more dangerous
state, because it looks complete.

## 3. What this change is NOT

- **Not a claim of wider coverage.** Tier 2 (rationals, inexact numbers) and Tier 3
  (effects, representation metadata) remain genuinely unimplemented in the FPGA path;
  this report records that fact, it does not create it.
- **Not a semantic change.** No parser, lower, compiler, or assembler line is modified.
  The only mutation is the test harness's accounting.
- **Not a contract bump.** `compatibility.my` and `language-contract.my` are untouched;
  CML stays at supported contract 2.0, exactly as before.

## 4. Result

The runner already asserts the contract gate fires (`unsupported_newer_contract > 0`,
line 409) — so the fail-closed *mechanism* exists for one axis. The gap is that the
other three axes (tier, float, unsupported-by-design) have no record at all. Writing
that record is the work here; it is mechanical, it is safe, and it is the difference
between "the test passes" and "the test proves what it says it proves."

---

# Відповідність CML — звіт про класифікацію "fail-closed" (2026-08-27) (Ukrainian)

**Завдання:** `CML-FULL-SEMANTIC-IR` (cml tasks.my, пріоритет 7.5)
**Обсяг:** Фаза 4 з `CML-CONTRACT-REALIGNMENT-M0` (Аналіз Viveka, §9 Фази 1-4).
**Чому саме цей зріз:** Фази 1-3 стосуються семантики `my-lisp` (контракт 2.1, 
вбудовані функції як значення першого класу) і є роботою на рівні авторитетного 
джерела поза цим репозиторієм. Фаза 4 — *зробити перевірку відповідності 
закритими за замовчуванням (fail closed)* — це зміна системи звітності, локальна 
для `cml`, і єдина частина етапу (milestone), яка не вимагає попередньої зміни 
контракту в іншому репозиторії. Це також є передумовою, що дозволяє перевіряти 
решту етапів.

**Живі докази:** `cargo test --test conformance_test` — **2 пройдено, 0 провалено, 101с**
(`tests/conformance_test.rs:203-413`, запущено з `cml/`).

---

## 1. Що раннер робить сьогодні (як побудовано, а не як заплановано)

`test_conformance` читає `../my-lisp/tests/fixtures/conformance.my`, проганяє 
кожну фікстуру Tier-1 через `парсинг → розширення макросів → lowering → 
компіляцію → асемблювання (fpga-lisp assembler.py) → симуляцію (vvp tb_cml_e2e.sv) 
→ декодування купи (heap)`, і перевіряє, що розкодоване значення дорівнює 
очікуваному `expected`. Фікстури з помилками перевіряють `RESULT_ERROR`.

Бар'єри допуску (admission gates), які зараз є в циклі (`conformance_test.rs:260-273`):

| Бар'єр | рядок | поведінка |
|---|---|---|
| Фільтр Tier | 261 | `if !line.contains("(tier . 1)") { continue; }` — **мовчки пропускає всі фікстури Tier-2 та Tier-3** |
| Бар'єр контракту | 265-268 | пропускає `since-contract > (2,0)`, рахуючи їх як `unsupported_newer_contract` |
| Бар'єр float | 271-273 | пропускає будь-який рядок, що містить `3.0` |

**Інвентаризація фікстур** (з `conformance.my`, 224 непорожні форми верхнього рівня):
- Tier 1: 35 — з яких реально виконуються 32 (3 пропущено через бар'єр float)
- Tier 2: 105 — **нуль виконується**
- Tier 3: 84 — **нуль виконується**

## 2. Чесна класифікація (Словник, необхідний для Фази 4)

Згідно з етапом, кожна фікстура має потрапити рівно в одну з категорій:

```
SUPPORTED              — виконана, значення збігається з очікуваним
UNSUPPORTED-BY-DESIGN  — виключена через назване, версіоноване апаратне обмеження
UNIMPLEMENTED          — входить в обсяг для бекенда, але ще не lowered/скомпільована
UNVERIFIED             — запущена, але не дала результату, що можна розкодувати
FAILED                 — виконана, але не збіглася з очікуваним результатом
```

Поточний раннер змішує перші три категорії в єдиний голий `continue` без запису. 
Майбутній читач (або другий бекенд) не може відрізнити "CML не підтримує Tier 2" від 
"тест просто не підключений до Tier 2" — і другий стан є небезпечнішим, оскільки 
виглядає так, ніби все завершено.

## 3. Чим ця зміна НЕ є

- **Не є заявою про розширене покриття.** Tier 2 (раціональні, неточні числа) та 
  Tier 3 (ефекти, метадані представлення) залишаються дійсно нереалізованими у 
  шляху FPGA; цей звіт фіксує цей факт, а не створює його.
- **Не є зміною семантики.** Жоден рядок парсера, модуля зниження, компілятора 
  чи асемблера не змінюється. Єдина зміна — це облік тестового середовища (test harness).
- **Не є підвищенням версії контракту.** `compatibility.my` та `language-contract.my` 
  не змінені; CML залишається на підтримуваному контракті 2.0, як і раніше.

## 4. Результат

Раннер вже перевіряє, що спрацьовує бар'єр контракту (`unsupported_newer_contract > 0`, 
рядок 409) — отже, *механізм* fail-closed існує для однієї з осей. Прогалина полягає 
в тому, що інші три осі (tier, float, unsupported-by-design) не мають жодного запису. 
Створення цього запису і є завданням; воно механічне, безпечне, і це та різниця 
між "тест проходить" і "тест доводить те, що, за його словами, він доводить".