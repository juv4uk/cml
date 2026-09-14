# FULL-SEMANTIC-IR: independent verification after `ce2d393`

**Date:** 2026-08-30  
**Scope:** read-only verification of the corrective change reported as
`ce2d3932cde0e2f50b4919306dc7a99c640c2485`  
**Current reviewed branch state:** `master` at `9100223b6b07f6cd9a90f2adef373f0a73a296b9`

## Verdict

The corrective change is real and fixes an important part of the semantic
debt, but the repository does **not yet have sufficient evidence to call
`CML-FULL-SEMANTIC-IR` finally verified**.

The narrow string-lowering correction and the C backend's quoted-value error
path are verified. The wider claim that all admitted semantic IR is handled
without backend panics is false in the reviewed state.

## Verified

### String lowering

`Expr::String(s)` now lowers to `Ir::String(s.clone())` in `src/lower.rs`.
It is no longer silently converted into `Quote(Sym(...))`. This preserves the
semantic distinction between a string and a symbol at the IR boundary.

### C backend quoted-value failure path

`CBackend::compile_quoted` now returns `Result<String, CompileError>`.
Unsupported quoted nodes return `CompileError::Unsupported` instead of
executing `unimplemented!`.

### Selected executable tests

The following bounded local tests passed against the reviewed tree:

```text
cargo test --test ir_lowering_test --test typed_buffer_ir_test

12 passed
0 failed
```

This evidence confirms the tested lowering and typed-buffer slices only. It
does not prove complete semantic coverage across every backend.

## Findings that block final verification

### 1. FPGA validation admits `Ir::Builtin`, but emission cannot handle it

`validate_ir` currently classifies this node as supported:

```rust
Ir::Nil | Ir::True | Ir::Var(_) | Ir::Builtin(_) => Ok(())
```

`compile_expr` has no `Ir::Builtin` arm. It reaches:

```rust
_ => unreachable!("unsupported IR node in compiler")
```

Therefore an admitted IR value can still reach a panic. This violates the
claimed fail-closed backend boundary.

Required resolution: either implement faithful FPGA emission for
`Ir::Builtin`, or reject it explicitly from `validate_ir` with a typed
`CompileError::Unsupported` until such emission exists.

### 2. Remaining `unreachable!` sites require an executable admission proof

The reviewed tree still contains backend `unreachable!` sites in
`src/compiler.rs`, including the wildcard arms for IR and quoted values.
An `unreachable!` behind a complete validator may be a defensible internal
invariant, but the `Ir::Builtin` mismatch proves that the current validator is
not complete enough to justify that invariant.

A regression test must enumerate or generate every `Ir`/`Quoted` variant and
prove that each is either emitted successfully or rejected with a typed error,
never a panic.

### 3. C conformance accounting accepts overly broad `Unsupported`

`tests/c_backend_conformance_test.rs` now counts every
`CompileError::Unsupported(_)` as an acceptable unsupported result and
continues.

That is weaker than the already recorded requirement in
`CML-CONFORMANCE-CLASSIFICATION-M2`: an error may count as unsupported only
when the fixture obligation and backend capability matrix predeclare the exact
reason. Otherwise a newly omitted implementation can make the suite green by
returning a generic `Unsupported`.

Required resolution: compare the backend's typed unsupported reason with an
explicit expected capability/reason for that fixture. Unexpected unsupported
results must be classified as failures.

### 4. Cleanup was not part of `ce2d393`

The commit modifies four source/test files and deletes no scripts. At review
time the working tree contains two unstaged deletions:

```text
D fix_assert.py
D rewrite.py
```

Consequently, the claim that all seven temporary scripts were removed by
`ce2d393` is not supported by that commit, and the repository is not currently
clean. These changes may be valid cleanup, but they need their own attributable
commit and verification.

### 5. Task status conflicts with the documented boundary

`tasks.my` marks `CML-FULL-SEMANTIC-IR` as done. However,
`docs/heterogeneous-backends.md` explicitly states that the current semantic
gate does not claim full my-lisp semantic analysis and that safe rejection is
not evidence that the full semantic-analysis milestone is complete.

The task must either be narrowed to a precisely proven representation and
classification milestone, or remain open until its present description is
actually satisfied.

## Evidence status

```text
STRING LOWERING FIX                 VERIFIED
C QUOTED-PATH PANIC REMOVAL         VERIFIED
SELECTED LOWERING/BUFFER TESTS       VERIFIED (12/12)
FPGA FAIL-CLOSED CLASSIFICATION      NOT VERIFIED
NO-PANIC FOR ALL ADMITTED IR         DISPROVED (`Ir::Builtin`)
CONFORMANCE UNSUPPORTED ACCOUNTING   TOO BROAD / REGRESSION RISK
TEMPORARY-SCRIPT CLEANUP             NOT COMMITTED IN `ce2d393`
FULL-SEMANTIC-IR                     NOT FINALLY VERIFIED
```

## Minimum closure conditions

1. Resolve the `Ir::Builtin` validator/emitter mismatch.
2. Add exhaustive admitted-IR-to-backend classification tests with panic
   detection.
3. Require exact, predeclared unsupported reasons in conformance accounting.
4. Commit and verify the intended cleanup separately.
5. Reconcile the task description/status with the narrower documented
   semantic boundary.
6. Run the relevant conformance suites and CI against the final commit, then
   record the exact commit SHA and test evidence.

Until all six conditions are met, the accurate claim is:

> The `ce2d393` correction materially improves semantic preservation and
> fail-closed handling, but it does not close or finally verify the full
> `CML-FULL-SEMANTIC-IR` milestone.

---

# FULL-SEMANTIC-IR: незалежна верифікація після `ce2d393` (Ukrainian)

**Дата:** 2026-08-30  
**Обсяг:** верифікація (лише читання) виправлення, повідомленого як
`ce2d3932cde0e2f50b4919306dc7a99c640c2485`  
**Поточний стан перевіреної гілки:** `master` на `9100223b6b07f6cd9a90f2adef373f0a73a296b9`

## Висновок

Виправлення є реальним і вирішує важливу частину семантичного боргу, але 
репозиторій **ще не має достатніх доказів, щоб назвати `CML-FULL-SEMANTIC-IR` 
остаточно верифікованим**.

Вузьке виправлення зниження (lowering) рядків та шлях обробки помилок 
для значень `quoted` у C-бекенді підтверджені. Більш широка заява про те, 
що всі допущені (admitted) семантичні вузли IR обробляються без панік 
(panic) бекенда, є хибною у перевіреному стані.

## Верифіковано (Підтверджено)

### Зниження рядків (String lowering)

`Expr::String(s)` тепер перетворюється на `Ir::String(s.clone())` у `src/lower.rs`. 
Він більше не перетворюється мовчки на `Quote(Sym(...))`. Це зберігає семантичну 
відмінність між рядком та символом на межі IR.

### Шлях помилок для quoted-значень у C-бекенді

`CBackend::compile_quoted` тепер повертає `Result<String, CompileError>`. 
Непідтримувані вузли `quoted` повертають `CompileError::Unsupported` замість 
виконання макросу `unimplemented!`.

### Окремі виконувані тести

Наступні локальні тести успішно пройшли на перевіреному дереві коду:

```text
cargo test --test ir_lowering_test --test typed_buffer_ir_test

12 passed
0 failed
```

Цей доказ підтверджує лише протестовані частини зниження (lowering) та 
буферів із типами (typed-buffer). Він не доводить повного семантичного 
покриття в усіх бекендах.

## Знахідки, що блокують остаточну верифікацію

### 1. Валідація FPGA допускає `Ir::Builtin`, але генерація (emission) не може її обробити

`validate_ir` зараз класифікує цей вузол як підтримуваний:

```rust
Ir::Nil | Ir::True | Ir::Var(_) | Ir::Builtin(_) => Ok(())
```

Функція `compile_expr` не має гілки для `Ir::Builtin`. Вона доходить до:

```rust
_ => unreachable!("unsupported IR node in compiler")
```

Отже, допущене значення IR все ще може призвести до паніки. Це порушує 
заявлену межу "fail-closed" для бекенда.

Необхідне рішення: або реалізувати правильну FPGA генерацію для `Ir::Builtin`, 
або явно відхиляти його з `validate_ir` за допомогою `CompileError::Unsupported`, 
поки така генерація не з'явиться.

### 2. Залишкові макроси `unreachable!` потребують виконуваного доказу допуску

Перевірене дерево все ще містить виклики `unreachable!` у `src/compiler.rs`, 
включно з fallback-гілками для IR та `quoted` значень. `unreachable!` за 
спиною повного валідатора може бути виправданим внутрішнім інваріантом, 
але невідповідність з `Ir::Builtin` доводить, що поточний валідатор 
недостатньо повний, щоб виправдати такий інваріант.

Регресійний тест має перебрати або згенерувати кожен варіант `Ir`/`Quoted` 
і довести, що кожен з них або успішно генерує код, або відхиляється з типізованою 
помилкою, але ніколи не викликає паніку.

### 3. Облік сумісності (conformance) C приймає занадто широке `Unsupported`

`tests/c_backend_conformance_test.rs` тепер рахує кожне 
`CompileError::Unsupported(_)` як прийнятний непідтримуваний результат 
і продовжує роботу.

Це слабше за вже зафіксовану вимогу в `CML-CONFORMANCE-CLASSIFICATION-M2`: 
помилка може вважатися непідтримуваною лише тоді, коли зобов'язання 
фікстури та матриця можливостей бекенда заздалегідь декларують точну причину. 
Інакше щойно вилучена реалізація може зробити тестовий набір зеленим, 
повертаючи загальне `Unsupported`.

Необхідне рішення: порівнювати типізовану причину `unsupported` від бекенда 
з явною очікуваною можливістю/причиною для цієї фікстури. Неочікувані 
непідтримувані результати повинні класифікуватися як failures.

### 4. Очищення не було частиною `ce2d393`

Цей коміт змінює чотири файли (з сирцями/тестами) і не видаляє жодних 
скриптів. На момент перевірки робоче дерево містить два непроіндексовані 
видалення (unstaged deletions):

```text
D fix_assert.py
D rewrite.py
```

Отже, заява про те, що всі сім тимчасових скриптів були видалені в `ce2d393`, 
не підтверджується цим комітом, і репозиторій наразі не є чистим. Ці зміни 
можуть бути правильним очищенням, але вони потребують окремого коміту та 
верифікації.

### 5. Статус завдання суперечить задокументованим межам

`tasks.my` позначає `CML-FULL-SEMANTIC-IR` як виконане. Однак 
`docs/heterogeneous-backends.md` прямо стверджує, що поточний семантичний 
бар'єр не претендує на повний семантичний аналіз my-lisp, і що безпечне 
відхилення не є доказом завершеності етапу повного семантичного аналізу.

Завдання має бути або звужене до точно доведеного представлення і 
класифікації, або залишатися відкритим, поки його поточний опис не буде 
фактично виконаним.

## Статус доказів

```text
ВИПРАВЛЕННЯ ЗНИЖЕННЯ РЯДКІВ          ВЕРИФІКОВАНО
УСУНЕННЯ ПАНІКИ C QUOTED-PATH        ВЕРИФІКОВАНО
ОБРАНІ ТЕСТИ LOWERING/BUFFER         ВЕРИФІКОВАНО (12/12)
FPGA FAIL-CLOSED КЛАСИФІКАЦІЯ        НЕ ВЕРИФІКОВАНО
ВІДСУТНІСТЬ ПАНІК ДЛЯ ВСІХ IR        СПРОСТОВАНО (`Ir::Builtin`)
C CONFORMANCE UNSUPPORTED ACCOUNTING ЗАНАДТО ШИРОКЕ / РИЗИК РЕГРЕСІЇ
ОЧИЩЕННЯ ТИМЧАСОВИХ СКРИПТІВ         НЕ ЗАКОМІЧЕНО В `ce2d393`
FULL-SEMANTIC-IR                     НЕ ВЕРИФІКОВАНО ОСТАТОЧНО
```

## Мінімальні умови для закриття

1. Вирішити невідповідність валідатора/генератора для `Ir::Builtin`.
2. Додати вичерпні тести класифікації допущених IR до бекенда з виявленням панік.
3. Вимагати точних, заздалегідь задекларованих причин `unsupported` в обліку 
   сумісності.
4. Закомітити та верифікувати заплановане очищення окремо.
5. Узгодити опис/статус завдання з вужчою задокументованою семантичною межею.
6. Запустити відповідні conformance-набори та CI проти фінального коміту, 
   після чого записати точний SHA коміту та докази тестів.

Доки не будуть виконані всі шість умов, точним твердженням є:

> Виправлення `ce2d393` суттєво покращує збереження семантики та 
> fail-closed обробку, але воно не закриває і остаточно не верифікує 
> повний етап `CML-FULL-SEMANTIC-IR`.
