# cml register ABI

The compiler emits fpga-lisp assembly directly, by hand, with no register
allocator — every `compile_*` function hardcodes which registers it reads
and writes. That convention has never been written down in one place
before this file, and its absence is the direct cause of three real bugs
found in one session (`e73f93a`, `166dffa`, and one still open — see
"Known gaps" below). This file is that missing specification.

## Register roles

| Register | Role | Lifetime |
|---|---|---|
| `R0` | Full evaluated argument list (for variadic/dotted lambda params) | Built fresh per call, in `compile_generic_call` |
| `R1`–`R3`, `R5`–`R9` | Argument registers (up to 8 fixed-arity params) *and* general scratch | **Not preserved across `compile_expr`/`compile_call`** — see below |
| `R4` | Current environment (an alist, innermost binding first) | Callee-switches on every closure invocation (`CDR R4 R15`); every primitive that doesn't itself call/lookup must leave it untouched |
| `R10` | Scratch for the closure's label pointer during a call (`CAR R10 R15`) | One call sequence only |
| `R11` | Shared software stack (cons-based, LIFO via `CONS`/`CAR`+`CDR`) | Program-lifetime; every push must have a matching pop in the same code path |
| `R12`, `R13` | Scratch for symbol IDs, NIL/TRUE construction, `cml_lookup`'s env cursor | **Not preserved across `compile_expr`** |
| `R14` | Link register: holds the return address a `RET R14` (or `RET R10`, `RET Rn`) jumps to | Caller sets before `CALL`/indirect `RET`; must be saved on `R11` before any nested call that needs its own value |
| `R15` | Return value convention; also `ATOM`'s output when building `T` | Whatever the last-executed primitive left there |

## The one rule that matters

**No register in `R0`–`R3`, `R9`, `R10`, `R12`, `R13`, `R15` survives a
call to `compile_expr`, `compile_call`, or `compile_generic_call` for a
*different* subexpression, regardless of that subexpression's own
`target_reg`.** Every primitive lowering (`+`, `cdr`, `eq`, `cons`,
`equal?`, symbol lookup, `cond`'s NIL-check, a nested call's argument/
closure evaluation) hardcodes some subset of these as scratch, so
compiling expression B after expression A can silently destroy a value A
already computed and left sitting in the register file — even if B's
`target_reg` is a different register than the one holding A's result.

If you need a value to survive compiling another subexpression, push it
onto `R11` first (`CONS R11 <reg> R11`) and pop it back
(`CAR <reg> R11` / `CDR R11 R11`) immediately after, in strict LIFO order
matching every other live push. This is the *only* mechanism this
codebase has for register preservation — there is no caller/callee-save
convention beyond "R4 and R14 get explicitly saved/restored around a
call; everything else you protect yourself if you need it."

`R4` is the one exception broad enough to name on its own: it is *not*
scratch anywhere, ever, including inside a primitive lowering that looks
unrelated to environments (see `e73f93a` below).

## Bugs this would have caught

- **`e73f93a`** — `compile_cond`'s strict-NIL truthiness check computed
  `NIL` into `R4` as scratch, on *every* branch evaluated (even ones not
  taken), which is exactly the one register this document says is never
  scratch. Any branch body needing an environment lookup ran against a
  destroyed environment. Fixed by using `R9` instead.
- **`166dffa`** — `compile_generic_call` evaluated call arguments one at
  a time into `arg_regs[i]`, but didn't protect argument `i`'s
  already-computed value before compiling argument `i+1` — and any
  primitive-call argument (`+`, `cdr`, ...) hardcodes `R1`–`R3` as its
  own scratch regardless of its `target_reg`. Fixed by pushing each
  argument onto `R11` immediately after computing it, not after all of
  them are computed.

## Known gaps

- ~~A 2+-parameter function invoked indirectly through a wrapper closure
  hangs if its body contains any function call~~ -- **retracted**. This
  was never a compiler or hardware bug: `fpga-lisp`'s shared
  `fpga/sim/tb_cml_e2e.sv` loads the program over a bit-banged UART at
  8680 time units/bit, so a ~200+-instruction binary alone takes
  ~70,000,000+ time units just to finish *loading*, before execution
  even starts -- right at or past the testbench's fixed 70,000,000-unit
  watchdog. Every case that looked like a hang (including the real
  `length`/`length-onto` pair from `core.my`, verified end-to-end
  through `my-lisp` -> `cml` -> `fpga-lisp`) completes correctly and
  produces the right `RESULT_VAL` once run with a larger watchdog --
  confirmed with a local instrumented testbench copy (PC trace + a
  200,000,000-unit watchdog), not by patching the shared repo. Flagged
  to `fpga-lisp` since this affects any sufficiently large program run
  through that testbench, not just `cml`'s output.
- This document was written by reading `compiler.rs`'s actual emitted
  instructions, not from an independent formal model — treat it as a
  description of present behavior to keep consistent, not as a
  guarantee that no other clobber-class bug remains unfound.

---

# cml регістровий ABI (Ukrainian)

Компілятор генерує асемблерний код `fpga-lisp` напряму, вручну, без розподільника
регістрів (register allocator) — кожна функція `compile_*` жорстко задає, які
регістри вона читає та записує. Ця конвенція ніколи раніше не була записана в
одному місці до появи цього файлу, і її відсутність стала прямою причиною
трьох реальних помилок, знайдених за одну сесію (`e73f93a`, `166dffa`, та
ще однієї відкритої — див. "Known gaps" нижче). Цей файл є тією відсутньою
специфікацією.

## Ролі регістрів

| Регістр | Роль | Тривалість життя (Lifetime) |
|---|---|---|
| `R0` | Повний обчислений список аргументів (для параметрів зі змінною кількістю / dotted lambda) | Створюється заново для кожного виклику в `compile_generic_call` |
| `R1`–`R3`, `R5`–`R9` | Регістри аргументів (до 8 параметрів фіксованої арності) *та* загальні тимчасові (scratch) | **Не зберігаються між `compile_expr`/`compile_call`** — див. нижче |
| `R4` | Поточне середовище (alist, найглибша прив'язка першою) | Перемикається (callee-switches) при кожному виклику замикання (`CDR R4 R15`); кожен примітив, що не робить викликів/пошуку, повинен залишати його незмінним |
| `R10` | Тимчасовий (scratch) для вказівника на мітку замикання під час виклику (`CAR R10 R15`) | Тільки на час однієї послідовності виклику |
| `R11` | Спільний програмний стек (на базі cons, LIFO через `CONS`/`CAR`+`CDR`) | На час життя програми; кожне заштовхування (push) має мати відповідне виштовхування (pop) у тому самому шляху виконання |
| `R12`, `R13` | Тимчасовий для ідентифікаторів символів, створення NIL/TRUE, курсора середовища в `cml_lookup` | **Не зберігаються між `compile_expr`** |
| `R14` | Link register: містить адресу повернення, куди стрибає `RET R14` (бо `RET R10`, `RET Rn`) | Викликач (caller) встановлює перед `CALL`/непрямим `RET`; має бути збережений у `R11` перед будь-яким вкладеним викликом, який потребує власного значення |
| `R15` | Конвенція значення повернення; також вивід `ATOM` при створенні `T` | Те, що залишив там останній виконаний примітив |

## Єдине правило, що має значення

**Жоден регістр у `R0`–`R3`, `R9`, `R10`, `R12`, `R13`, `R15` не переживає 
виклик `compile_expr`, `compile_call` або `compile_generic_call` для 
*іншого* підвиразу, незалежно від власного `target_reg` цього підвиразу.** 
Кожне зниження (lowering) примітива (`+`, `cdr`, `eq`, `cons`, `equal?`, 
пошук символу, перевірка на NIL у `cond`, обчислення аргументу/замикання 
вкладеного виклику) жорстко закодовує певну підмножину з них як тимчасові 
(scratch), тому компіляція виразу B після виразу A може непомітно знищити 
значення A, яке вже обчислене і лежить у регістровому файлі — навіть якщо 
`target_reg` для B відрізняється від регістра, що містить результат A.

Якщо вам потрібно, щоб значення пережило компіляцію іншого підвиразу, 
спочатку збережіть його в `R11` (`CONS R11 <reg> R11`) і дістаньте назад 
(`CAR <reg> R11` / `CDR R11 R11`) одразу після цього, у строгому порядку 
LIFO, що відповідає кожному іншому активному push. Це *єдиний* механізм 
для збереження регістрів у цій кодовій базі — тут немає жодної конвенції 
caller/callee-save, окрім "R4 та R14 явно зберігаються/відновлюються 
навколо виклику; решту ви захищаєте самі, якщо вони вам потрібні."

`R4` є єдиним винятком, достатньо широким, щоб назвати його окремо: він *не*
є тимчасовим (scratch) ніде й ніколи, навіть усередині зниження примітива, 
яке виглядає не пов'язаним із середовищами (див. `e73f93a` нижче).

## Помилки, які це дозволило б знайти

- **`e73f93a`** — перевірка на строгу істинність `NIL` у `compile_cond` 
  обчислювала `NIL` у `R4` як тимчасовий регістр на *кожній* обчисленій 
  гілці (навіть на тих, що не виконувалися), що є саме тим єдиним регістром, 
  який за цим документом ніколи не буває тимчасовим. Будь-яке тіло гілки, 
  яке потребувало пошуку в середовищі, працювало зі знищеним середовищем. 
  Виправлено шляхом використання `R9`.
- **`166dffa`** — `compile_generic_call` обчислював аргументи виклику 
  по черзі в `arg_regs[i]`, але не захищав уже обчислене значення 
  аргументу `i` перед компіляцією аргументу `i+1` — а будь-який аргумент 
  примітивного виклику (`+`, `cdr`, ...) жорстко закодовує `R1`–`R3` як 
  власні тимчасові регістри незалежно від свого `target_reg`. Виправлено 
  заштовхуванням кожного аргументу в `R11` одразу після його обчислення, 
  а не після того, як будуть обчислені всі.

## Відомі прогалини

- ~~Функція з 2+ параметрами, викликана опосередковано через замикання-обгортку,
  зависає, якщо її тіло містить будь-який виклик функції~~ -- **відкликано**. 
  Це ніколи не було помилкою компілятора чи обладнання: спільний тестовий стенд 
  `fpga/sim/tb_cml_e2e.sv` у `fpga-lisp` завантажує програму через bit-banged UART
  зі швидкістю 8680 одиниць часу/біт, тому лише один бінарник розміром ~200+ 
  інструкцій потребує понад ~70,000,000 одиниць часу просто для завершення 
  *завантаження*, ще до початку виконання -- що якраз перевищує фіксований 
  на стороні тестового стенда 70,000,000-unit watchdog. Кожен випадок, який виглядав 
  як зависання (зокрема реальна пара `length`/`length-onto` з `core.my`, перевірена 
  наскрізно через `my-lisp` -> `cml` -> `fpga-lisp`), завершується коректно і 
  видає правильний `RESULT_VAL` після запуску з більшим значенням watchdog -- 
  це підтверджено локальною інструментованою копією тестового стенда (PC trace + 
  200,000,000-unit watchdog), а не шляхом патчингу спільного репозиторію. 
  Про це повідомлено `fpga-lisp`, оскільки це впливає на запуск будь-якої 
  достатньо великої програми через цей тестовий стенд, а не лише на вивід `cml`.
- Цей документ був написаний на основі читання фактично згенерованих інструкцій
  у `compiler.rs`, а не з незалежної формальної моделі — сприймайте його як 
  опис поточної поведінки для підтримання узгодженості, а не як гарантію, що 
  не залишилося жодної іншої помилки класу "затирання регістра" (clobber-class bug).
