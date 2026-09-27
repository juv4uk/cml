# ADR-006: Межі 8-бітної ідентичності — насичення простору й відсутність ідентичності в `Closure`

**Status:** ACCEPTED — розподіл namespace **вирішено: варіант (б) «256 = лише
примітиви»** (ратифіковано власником 2026-09-27)
**Date:** 2026-09-27
**Authors:** cml team (діагностика); рішення про namespace — власник
**Related Issue:** [#315](https://github.com/juv4uk/cml/issues/315) (`SENS-FIRST`)
**Related:** [ADR-005](2026-09-27-result-form-authority-pinned-corpus.md)
**Context:** Власник сформулював цінність SENS («стабільність, компактність,
переносимість») і запропонував наступний крок. Агент перевірив твердження по
коду і знайшов два конструктивні обмеження, які варто зафіксувати **до**
наступного кроку, а не після.

[Українська](#українська) · [English](#english)

---

## Українська

### 1. Що підтвердилося: прибрано залежність від людського імені

Твердження власника перевірено емпірично, і воно стосується не гіпотези, а
закоміченого факту.

**Доказ із цієї сесії (ADR-005).** Коли `builtin_atom` у CML повертав
`(STRUCTURAL-KIND ATOM)`, а пінований корпус вимагав `(1)`, суперечку **неможливо
було розв'язати читанням імен**. Ім'я `atom` не дало нічого; розв'язок дав один
рядок у фікстурі корпусу: `(00000010 (quote ())) (expected . "()")`.

**Сильніше за «компактно»:** 8-бітний код зробив розбіжність **читабельною**.
Коли обидва боки звучали правдоподібно (`(STRUCTURAL-KIND ATOM)` проти `(1)`),
різниця була неочевидною. З кодом вона видима з першого погляду на рядок
фікстура. Отже, компактність ідентичності — не лише економія місця; вона робить
розбіжність **помітною**. Це і є зміст слова «стабільність».

### 2. Вимірювання існує і воно добре спроєктоване

Джерело: `external/sens/benchmarks/sens-surface/results/20260927-three-way/README.md`
(експеримент 2026-09-27). Інструкції процесора під valgrind (cachegrind
`I refs`), 11 навантажень, 3 повтори, медіана, розкид ≤ 0,71 %.

Ключова перевага дизайну: **`new-English` і `new-SENS` міряються буквально тим
самим бінарником `5cff80e6`.** Єдина відмінність — представлення імені. Це
прямо перевіряє гіпотезу власника, а не порівнює два різні компілятори.

| порівняння | результат |
|---|---|
| SENS проти англійських імен того самого бінарника | **×1,195 (+19,5 %)** |
| SENS проти англійського Lisp 2026-09-08 | ×1,147 (+14,7 %) |
| завантаження: двійковий SENS проти тексту | **×3,76** |
| `fib` | ×1,323 |
| `loop` | ×1,40 |
| `ackermann` | ×1,302 |
| `closures` | ×1,293 |
| `assoc` | ×1,045 |

Останній рядок важливіший за інші: **виграш нерівномірний** — від ×1,045 до
×1,40. Він великий там, де ім'я розв'язується в гарячому циклі.

Автор експерименту чесно виправив власну першу версію: вона подавала SENS
текстом (код — вісім знаків «0/1»), що суперечить аксіомам 3–5 і 14. Виправлення
переведено на двійковий вигляд (fasl), де функція SENS займає ровно 1 байт, а
англійське ім'я — тег, 4-байтова довжина й рядок.

**Умова, яку не можна забути:** `[profile.release] opt-level = "z"`, `lto = true`.
`"z"` оптимізує **розмір, не швидкість**. Відносні порівняння чесні; абсолютні
числа — не межа можливого.

### 3. Межа, яку власник назвали правильно, — вона глибша за Runtime

Власник правильно вказав на `Rc<str> → HashMap` і `Vec<Rc<str>>`. Але це
**недолік конструкції мови, а не лише проблема представлення.**

```rust
// external/sens/crates/sens/src/value.rs:399
pub struct Closure {
    pub(crate) parameters: Vec<Rc<str>>,
    pub(crate) rest: Option<Rc<str>>,
    pub(crate) body: Rc<[Expr]>,
    pub(crate) environment: Environment,
}

// external/sens/crates/sens/src/environment.rs:28
values: HashMap<Rc<str>, Value>,
```

**У `Closure` немає поля ідентичності.** `def` і `lambda` отримують `Sens8`
(`necessary_forms.rs:54`), але сама визначена функція — ні. Тому вона
зберігається за ім'ям у `HashMap<Rc<str>, Value>`, а ім'я тримається в пам'яті,
хешується й порівнюється на кожному виклику.

**Наслідок:** наступний крок «local variable → lexical slot» — це не оптимізація.
Це закриття другої половини розрізнення. Мова зняла імена з **виклику**,
але не з **визначення** та **показування**. Розрізнення `meaning`/`spelling`
на сьогодні зняте наполовину.

### 4. Простір насичено — це не деталь, це блокер наступного кроку

| показник | значення |
|---|---|
| рядків у `semantic-registry.lisp` | 256 |
| унікальних кодів | **256** |
| вільних кодів | **0** |

Зайнято буквально все, включно з вузькими іменами:

| код | `en` | стан |
|---|---|---|
| `10110001` | `answer-not` | зайнятий |
| `10110010` | `answer-and` | зайнятий |
| `10110011` | `answer-or` | зайнятий |
| `10110100` | `answer-weaken` | зайнятий |
| `10110101` | `answer-atom` | зайнятий |
| `10110110` | `answer-eq` | зайнятий |

Це пряма суперечність із Lisp-спадком: Lisp — мова, де визначення функцій є
основним act програмування. Але для наступної такої функції місця немає.

Тому наступний крок — не просто `lexical slot`, а відповідь на питання, яке
треба вирішити **перед** ним.

### 5. Три варіанти, і наслідок кожного

| варіант | що дає | ціна |
|---|---|---|
| **(а) `Sens16`** | простір знову відкритий (65 536) | твердження «8 біт = одне слово» більше не працює; втрачається FPGA-стиснення та половина переваги з §2 |
| **(б) 256 — це лише примітиви** | чесна межа; `Sens8` залишається 1 байтом | dispatch на рівні визначень лишається іменованим; треба проговорити явно |
| **(в) гібрид** | примітиви мають код, визначені функції — компактний індекс у таблиці компіляції | два механізми ідентичності; `Sens8` уточнює лише примітиви |

Рекомендація — **(б)**, бо він єдиний не суперечить виміряним числам у §2.

**Рішення власника 2026-09-27: (б) прийнято.** Наслідок, який тепер є
обов'язковим: межа `Sens8` проголошується явно — **`Sens8` позначає примітив, а
не функцію**; визначені функції ідентифікації в `Sens8` не мають і dispatch для
них лишається іменованим. Це має бути записано в документації мови, а не
лишитися неявним припущенням у коді.

### 6. Наслідок для твердження «перевага стане вимірюватися разами»

**Твердження прийнятне, але з одним уточненням: воно стосується примітивного
ядра, а не мови загалом.**

- Підтверджено: гарячий dispatch дає до ×1,40.
- Спірне: «рази» на загальних workload — ні, `assoc` лише ×1,045.
- Найважливіше: **`fact`, `map`, `fold` у реальному коді — це визначені
  функції**, тобто names-based path за варіантом (б). Отже, очікування
  «рази» **не поширюється** на код, який пишуть користувачі, якщо обрано (б).

### 7. Перевірюваний висновок, а не лише гіпотеза

Найцікавіше спостереження §2 у поєднанні з §3: навантаження **`closures` отримало
×1,293** — і при цьому `Environment` досі `HashMap<Rc<str>, Value>`, а `Closure`
 досі не має ідентичності.

Тобто ×1,293 було отримано **попри** імена, які й досі живуть у середовищі.
Отже, усунення імен із `Closure`/`Environment` має **вимірювану передбачену
верхню межу**, а не лише теоретичну перевагу.

Це робить наступний крок **фальсифікованим**: якщо після усунення імен
`closures` не зросте далі, гіпотеза «SENS дає перевагу здешевлянням імен»
для цього workload неправильна.

### 8. Виправлення власних помилок агента

За принципом «правда важливіша за красивий звіт»:

1. Агент написав, що `answer-atom?` = `183`, `answer-eq?` = `184`. Це
   **номери рядків**, а не коди. Реально: `answer-atom` = `10110101`,
   `answer-eq` = `10110110`. Також у коді немає `answer-atom?` — лише
   `answer-atom` (без знака питання).
2. Агент заявив, що джерела `+20–30 %` не знайдено. **Помилка**: джерело є,
   воно рівно те, що наведено в §2. Агент шукав у `docs/reports/` і не знайшов,
   бо звіт лежить у `benchmarks/sens-surface/results/`.
3. Агент описав `honest_c_vs_lisp_benchmark` і
   `diagnostic_performance_benchmark_on_large_buffer` як окремі файли тестів.
   Це не так: це функції тестів у `tests/countdown_bench_test.rs:7` і
   `tests/cpu_parallel_compute_backend_test.rs:197`. Обидві зараз червоні.

### 9. Що це ADR фіксує, а що — ні

**Фіксує:**
- `lexical slot` — конструктивна необхідність, а не оптимізація;
- `Sens8` насичено (256/256), наступна функція не має коду;
- «рази» стосується гарячого dispatch примітивного ядра, не мови загалом;
- `Closure` не має ідентичності — це дефект конструкції, а не Runtime;
- **обрано варіант (б): `Sens8` = примітив, не функція** (§5).

**Не фіксує (відкриті):**
- механізм ідентифікації визначених функцій — за (б) він лишається іменованим,
  і це має бути спроєктовано, а не залишено як випадковість;
- чи буде `Sens8` розширено;
- будь-яка оцінка швидкості **в CML** — її вимірювання ще не існує.

**Про CML чесно:** жодного валідного вимірювання швидкості в CML на сьогодні
немає. Наші бенчмарки червоні. Числа §2 — з `sens`, не з `cml`.

---

## English

### 1. Confirmed: execution is no longer name-dependent

Verified empirically, not by argument. In this session two committed authorities
disagreed about what `00000010` returns. The conflict **could not be resolved by
reading names**: the name `atom` told us nothing; one row of the pinned corpus
did (`(00000010 (quote ())) (expected . "()")`).

**Stronger than "compact":** the 8-bit code made the divergence **legible**. When
both sides sounded plausible (`(STRUCTURAL-KIND ATOM)` vs `(1)`), the difference
was not obvious. With a code it is visible at a glance. Compactness is therefore
not merely economy of space; it makes disagreement **noticeable**. That is what
"stability" means here.

### 2. The measurement exists, and it is well designed

Source: `external/sens/benchmarks/sens-surface/results/20260927-three-way/README.md`
(2026-09-27). CPU instructions under valgrind (cachegrind `I refs`), 11
workloads, 3 runs, median, spread ≤ 0.71 %.

Key design strength: **`new-English` and `new-SENS` are measured in literally the
same binary `5cff80e6`.** The only difference is how the name is represented. This
tests the hypothesis directly rather than comparing two different compilers.

| comparison | result |
|---|---|
| SENS vs English names, same binary | **×1.195 (+19.5 %)** |
| SENS vs English Lisp 2026-09-08 | ×1.147 (+14.7 %) |
| load: binary SENS vs text | **×3.76** |
| `fib` | ×1.323 |
| `loop` | ×1.40 |
| `ackermann` | ×1.302 |
| `closures` | ×1.293 |
| `assoc` | ×1,045 |

The last row matters more than the rest: **the gain is uneven**, ×1.045 to ×1.40,
and it is large exactly where names are resolved in a hot loop.

The author honestly corrected a first version that fed SENS as *text* (eight
`0`/`1` characters), which contradicts axioms 3–5 and 14; the correction uses
binary form (fasl) where a SENS function is exactly 1 byte.

**Condition that must not be forgotten:** `opt-level = "z"`, `lto = true`. `"z"`
optimizes for **size, not speed**. Relative comparisons are honest; absolute
numbers are not the ceiling.

### 3. The limit the owner named is deeper than Runtime

`Closure` (`value.rs:399`) has **no identity field**; `Environment`
(`environment.rs:28`) is `HashMap<Rc<str>, Value>`. `def` and `lambda` get a
`Sens8`, but the defined function itself does not — so it is stored *by name*, and
the name is hashed and compared on every call.

So "local variable → lexical slot" is not an optimization. It is closing the
**second half** of the separation. The language has removed names from *calling*,
but not from *definition* and *display*.

### 4. The space is saturated — a blocker, not a detail

256 rows, **256 unique codes, 0 free**, including narrow entries such as
`answer-atom` = `10110101` and `answer-eq` = `10110110`. There is no room for the
next function. Since defining functions is the core act of Lisp, this is a direct
tension with Lisp heritage.

### 5. Three options and the cost of each

| option | gain | cost |
|---|---|---|
| **(a) `Sens16`** | space reopens (65 536) | "8 bits = one word" stops holding; FPGA compression and half of §2 are lost |
| **(b) 256 = primitives only** | honest boundary; `Sens8` stays 1 byte | dispatch over definitions stays name-based; must be stated explicitly |
| **(c) hybrid** | primitives coded, defined functions get a compact table index | two identity mechanisms |

Recommendation: **(b)** — the only option that does not contradict §2.

### 6. Consequence for "the advantage will be measured in times"

**Acceptable, with one correction: it applies to the primitive core, not to the
language as a whole.** Hot dispatch reaches ×1.40; `assoc` is only ×1,045. More
importantly `fact`, `map`, `fold` in real code are *defined functions* — the
name-based path under option (b). So "times" **does not extend** to
user-written code.

### 7. A falsifiable consequence, not just a hypothesis

The `closures` workload gained **×1.293 while `Environment` is still
`HashMap<Rc<str>, Value>` and `Closure` still has no identity** — i.e. the gain
was obtained *despite* names still living in the environment. Removing those
names therefore has a **measurable predicted upper bound**.

This makes the next step falsifiable: if `closures` does not improve further after
the names are removed, the hypothesis "SENS gains by making names cheap" is wrong
for that workload.

### 8. Agent self-corrections

1. The agent wrote `answer-atom?` = `183`, `answer-eq?` = `184`. Those are **line
   numbers**, not codes; the codes are `10110101` / `10110110`. There is also no
   `answer-atom?` in the registry — only `answer-atom`.
2. The agent claimed the `+20–30 %` figures had no traceable source. **Wrong**:
   the source is exactly §2; the agent searched `docs/reports/` and missed
   `benchmarks/sens-surface/results/`.
3. The agent described `honest_c_vs_lisp_benchmark` and
   `diagnostic_performance_benchmark_on_large_buffer` as separate test files.
   They are test functions in `tests/countdown_bench_test.rs:7` and
   `tests/cpu_parallel_compute_backend_test.rs:197`. Both are currently red.

### 9. What this ADR fixes, and what it does not

**Fixes:** lexical slot is a construct necessity; `Sens8` is saturated
(256/256); "times" concerns hot dispatch of the primitive core; `Closure` lacks
identity as a design defect rather than a runtime issue.

**Does not fix (open):** which option of §5 is chosen; whether `Sens8` is extended;
any speed figure **in CML**, which does not yet exist. Honestly: CML has no valid
speed measurement to date; our benchmarks are red, and §2's numbers come from
`sens`, not `cml`.
