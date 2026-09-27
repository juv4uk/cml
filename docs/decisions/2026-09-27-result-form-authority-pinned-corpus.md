# ADR-005: Форма результату для `00000010` / `00000011` — пінований корпус є authority

**Status:** ACCEPTED
**Date:** 2026-09-27
**Authors:** cml team (рішення власника)
**Related Issue:** [#315](https://github.com/juv4uk/cml/issues/315) (`SENS-FIRST`)
**Context:** Суперечність між двома закомітованими авторитетами щодо форми результату тотожності.

[Українська](#українська) · [English](#english)

---

## Українська

### 1. Контекст: розбіжність, яка була закомітована

Під час переходу на `sens::Sens8` як єдину 8-бітну ідентичность мовної функції
виявилося, що **два закомітовані джерела в репозиторії дають несумісні відповіді**
про те, що саме повертають коди `00000010` та `00000011`.

`external/sens/lib/surface/semantic-registry.lisp` однозначно називає їх:

| код | `en` | `ук` | `sym` |
|---|---|---|---|
| `00000010` | `atom?` | `атом?` | `.?` |
| `00000011` | `eq?` | `тотожне?` | `=?` |
| `00100001` | `not?` | `хибне?` | `(())` |

Реєстр визначає **ідентичність** коду, але не визначає **форму результату**.
Остання мала два несумісні джерела.

**Джерело A — пінований конформанс-корпус** (`external/sens/tests/fixtures/`,
канонізований через `(repository . "juv4uk/sens")` у `upstream-revisions.lisp`):

```lisp
(00000010 (quote ()))              ; expected (0)-подібне: порожній список → ()
(00000010 (quote (radio antenna))) ; expected (0)          ; cons → (0)
(00000010 (quote radio))           ; expected (1)          ; symbol → (1)
(00000011 (quote radio) (quote radio))  ; expected (1)
(00000011 (quote radio) (quote antenna)) ; expected (0)
(00000011 3 3)                          ; expected (1)
(00000011 3 3.0)                        ; expected (1)  ; крос-типове числове порівняння
(00000011 3.0 3.0)                      ; expected (1)
(equal? (quote (1 2)) (quote (1 2 3)))  ; expected (0)
```

**Джерело B — власні коміти CML.** `src/c_backend.rs` на HEAD реалізував
`builtin_atom` через `v_structural_kind()`, а `builtin_eq`/`builtin_equal_p` —
через `identity_relation()`/`structural_relation()`, повертаючи дескриптори:

```text
(00000010 (quote x))   →  (STRUCTURAL-KIND ATOM)
(00000011 x x)         →  (IDENTITY-RELATION SAME)
```

Власні тести CML (`tests/closure_parity_test.rs` — 26 комітів, `tests/c_backend_test.rs`,
`tests/language_libraries_test.rs`) були повністю узгоджені з діалектом B: тричасткові
`cond`-клаузи зіставляли `(identity-relation same)` / `(structural-kind pair)`.

Тобто репозиторій містив **дві внутрішньо узгоджені, але взаємно суперечливі семантики
для тих самих двох ідентичностей**. Обидві були закомітовані.

### 2. Рішення

**Пінований корпус є authority для форми результату.**

Рішення власника, 2026-09-27: «Корпус — authority: (1)/(0)/()».

Обґрунтування в межах наявних аксіом:

- `upstream-revisions.lisp` declares `supported-pin` як
  `compatibility-claim-denominator` і `build-source-does-not-promote-supported-contract`.
  Саме конформанс-корпус є знаменником сумісності, а не власний код CML.
- Принцип «бекенд не визначає семантику» означає, що реалізація CML не може
  перевизначати форму результату, зафіксовану корпусом.
- Діалект дескрипторів не має жодного рядка в корпусі — отже він не є
  зафіксованим контрактом, а локальною конвенцією реалізації.

### 3. Наслідки для CML

`src/c_backend.rs` переведено на контракт корпусу:

- `builtin_atom` (`00000010`): `()` для `nil`, `(1)` для атома, `(0)` для `cons`.
- `builtin_eq` (`00000011`): `(1)` / `(0)`, з крос-типовим числовим порівнянням
  `int`/`rational` через `rational_checked_mul` (вимога рядка
  `(00000011 3 3.0) → (1)`).
- `builtin_equal_p`: `(1)` / `(0)` через `v_equal_p`.

Хелпери `v_structural_kind`, `identity_relation`, `structural_relation` та
`relation_record` видалено: після переходу вони не мали жодного виклика, і
залишати мертвий код, що суперечить обраному контракту, було б дезорганізацією.

Тести, записані в діалекті дескрипторів, переписано на truthy-діалект:
`(structural-kind pair)` → `(0)`, `(identity-relation same)` → `(1)`, тощо.
Це стосується `tests/closure_parity_test.rs`, `tests/language_libraries_test.rs`,
`tests/bounded_memory_test.rs`, `tests/c_backend_test.rs`,
`tests/triple_oracle_test.rs`, `tests/canonical_cond_current_contract_test.rs`,
`tests/conformance_matrix_test.rs`, `tests/explicit_cond_backend_test.rs`,
`tests/explicit_cond_dispatch_test.rs`, а також продуктного рядка
`lisp_source` у `src/native_baseline.rs` (робоче навантаження `counted-loop`).

Ім'я файлу `contracts/core4/eq-identity-relation.lisp` — це **шлях**, а не
значення, і воно навмисно не змінювалося.

### 4. Виміряний результат

| стан | passed | failed |
|---|---|---|
| HEAD (до роботи) | 468 | 109 |
| після реставрації діалекту B | 558 | 29 |
| після рішення (діалект корпусу) | 562 | 25 |

`tests/closure_parity_test.rs` 26/26, `tests/c_backend_test.rs` 10/10,
`tests/language_libraries_test.rs` 6/6, `tests/explicit_cond_dispatch_test.rs` 4/4.

### 5. Що залишається OPEN

1. **FPGA-шлях розходиться окремо.** `tests/conformance_test.rs` ганяє
   `parse → MacroExpander → lower → Compiler → asm` і **не торкається
   `src/c_backend.rs`**. Він повертає `t` / `()`, а не `(1)` / `(0)`.
   Це третій, незалежний divergence — виправлення потребує окремої роботи
   над `src/compiler.rs` та FPGA-семантикою. Це **не** застарілий корпус.
2. **Публікація `atom?` / `atom`.** Реєстр publishes `atom?`, а `C1-PRIMITIVE-IDENTITY`
   вживає `atom`. Чотири тести (`admitted_first_class_callable_values_are_exact_sens8`,
   `every_admitted_canon_callable_surface_*`, `rejects_def_of_latin_canon_names`,
   `sens_codes_are_the_basis_not_names`) залишаються червоними. Це той самий
   спадок #315, який вимагає unified reader/semantic resolver.
3. `c_backend_executes_canonical_cond_by_private_structural_match` падає на
   **gcc/Guix glibc header**, а не на семантиці.
4. `WSM_MY_LISP_CORE1_PRELUDE_SOURCE` не заданий (3 тести) і sibling-репозиторій
   `/home/agents/GitHub/my-lisp` відсутній (2 тести).

### 6. Наслідок для пам'яті

Правило, яке слід запам'ятати: **коли два коміти репозиторію суперечать один
одному щодо семантики, перевіряй пінований upstream-корпус перш за все, і
визнавай контракт з нього, а не з власного коду.**

---

## English

### 1. Context

During the migration to `sens::Sens8` as the single 8-bit identity for a
language function, two **committed** sources in this repository were found to
disagree about the *result form* of `00000010` and `00000011`.

`semantic-registry.lisp` names them unambiguously: `00000010 = atom?`,
`00000011 = eq?`, `00100001 = not?`. The registry fixes identity, not result
form.

**Source A — the pinned conformance corpus** (`external/sens/tests/fixtures/`,
canonised as `(repository . "juv4uk/sens")` in `upstream-revisions.lisp`)
expects `(1)` / `(0)`, with `atom?` of `()` being `()`.

**Source B — CML's own commits.** HEAD's `src/c_backend.rs` routed
`builtin_atom` through `v_structural_kind()` and `builtin_eq` through
`identity_relation()`, returning `(STRUCTURAL-KIND ATOM)` /
`(IDENTITY-RELATION SAME)`. CML's own 26 committed closure-parity tests were
fully consistent with dialect B, matching `(identity-relation same)` and
`(structural-kind pair)` in three-part `cond` clauses.

The repository therefore carried two internally consistent but mutually
incompatible semantics for the same two identities.

### 2. Decision

**The pinned corpus is the authority for result form.** Owner ruling,
2026-09-27.

`supported-pin` is declared the `compatibility-claim-denominator`, and a
backend does not define semantics. Dialect B appears nowhere in the corpus,
so it is a local implementation convention rather than a fixed contract.

### 3. Consequence

`src/c_backend.rs` now implements the corpus contract: `builtin_atom` returns
`()`, `(1)` or `(0)`; `builtin_eq`/`builtin_equal_p` return `(1)`/`(0)` with
cross-type int/rational comparison, as row `(00000011 3 3.0) → (1)` demands.
The now-unreferenced descriptor helpers were deleted rather than left dead.
Tests written in dialect B were rewritten to the truthy dialect.

### 4. Measured

| state | passed | failed |
|---|---|---|
| HEAD | 468 | 109 |
| after restoring dialect B | 558 | 29 |
| after the ruling (corpus dialect) | 562 | 25 |

### 5. Still OPEN

`tests/conformance_test.rs` drives the **FPGA** pipeline and never touches
`src/c_backend.rs`; it yields `t`/`()` — a third, independent divergence
requiring separate work. The `atom?`/`atom` publication split (four tests) is
the inherited #315 blocker. One test fails on a gcc/Guix glibc header.
Three require `WSM_MY_LISP_CORE1_PRELUDE_SOURCE`; two require the absent
sibling repository `/home/agents/GitHub/my-lisp`.
