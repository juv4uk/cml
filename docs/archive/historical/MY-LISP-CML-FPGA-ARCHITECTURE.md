# my-lisp, CML, and fpga-lisp: one semantic system

Status: architecture analysis, 2026-08-24. This document records the
relationship observed in the three live repositories; machine-readable
contracts remain authoritative over this prose.

## Roles

```text
my-lisp    defines what a program means
CML        lowers that meaning into executable forms
fpga-lisp  physically executes one of those forms
```

`my-lisp` is the semantic authority. CML is the integration point, not a
second definition of Lisp. `fpga-lisp` is both an ISA target for CML AOT
programs and an independent hardware evaluator. Those two FPGA roles must
remain distinguishable:

```text
CML -> assembly -> fpga-lisp ISA     compiled execution
Lisp data -> hardware eval/apply     independent semantic implementation
```

The first path tests the compiler. The second tests the universality of the
language and prevents CML and the FPGA backend from agreeing on the same
incorrect interpretation.

## Current contract boundary

| Component | Declared boundary | Observed state |
|---|---|---|
| my-lisp | language contract 3.0 | semantic source of truth |
| CML | language 2.0, partial capability admission from 2.1 | common IR, C and FPGA emitters |
| fpga-lisp | ISA 1.1 | 32-bit tagged Lisp machine with extended register-input frame |
| CML -> FPGA | language 2.0 / ISA 1.1 | real assembler, transport, and hardware path |

The FPGA path already supports tagged values, a 4096-cell cons heap, a
4096-word program image, sixteen registers, the McCarthy primitives,
ADD/SUB, control flow, calls, closures, lexical environments, variadic
arguments, recursive definitions, and compiled examples including
`reverse`, `append`, and structural `equal?`.

## Target architecture

```text
                     my-lisp
              versioned language contract
                         |
                 canonical fixtures
                         |
                         v
                  CML semantic IR
                         |
             analysis + representation
              +----------+----------+
              |                     |
          C runtime          FPGA machine IR
                                    |
                           register allocation
                                    |
                            shared execution ABI
                                    |
                              fpga-lisp ISA
                                    |
                                  RTL
```

GPU and FPGA dataflow work belongs beside this path, not inside the language
semantics:

```text
CML semantic IR
  +-- general Lisp lowering -> C runtime / fpga-lisp machine
  `-- compute/dataflow IR   -> GPU / specialized FPGA pipeline
```

Thus `fpga-lisp` is not merely "another GPU-like backend." It can execute
general Lisp through its machine ISA, while a future FPGA dataflow backend
could specialize pure stream or pipeline regions without an evaluator.

## Important seams to formalize

### First-class builtins

Language contract 2.1 makes builtins ordinary callable values that may be
passed and lexically shadowed. CML's C backend has this capability, while the
FPGA emitter still lowers primitive calls directly. The hardware already has
`TAG_PRIMITIVE`; the next design must bootstrap primitive values into the
environment and make both the hardware evaluator and CML-generated calls use
the same lookup/apply rule.

### Execution ABI

CML currently documents the effective FPGA calling convention: R0 carries
the complete argument list, R4 the environment, R11 the cons-based software
stack, R14 the link, and R15 the result. This is a contract between two repos,
not merely a private implementation note. It should become a machine-readable
shared ABI checked by both CML and fpga-lisp.

### Numeric representation

my-lisp promises arbitrary-precision exact integers and exact normalized
rationals. FPGA currently has a fixnum payload, and CML additionally limits
literals to the LOADI immediate range. The proposed boxed sign-magnitude
bignum and numerator/denominator rational representations are directionally
sound, but must be ratified through fixtures before RTL. The representation
draft also says only two tag slots remain although ISA 1.0 names six of sixteen
tag values; that statement needs correction or an explicit account of other
reserved tags.

The safe sequence is:

```text
my-lisp fixtures
-> representation contract
-> ISA tag revision
-> RTL constructors/accessors
-> arithmetic routines
-> CML representation lowering
-> differential conformance
```

### Heap and garbage collection

R11's stack is itself a cons list in the same 4096-cell bump-allocated heap.
Calls therefore consume heap even after logical pops. `SETCDR` also permits
cycles through recursive closure environments, so reference counting is not
sufficient. A tracing collector is a runtime-correctness milestone for larger
compiled programs, not merely a performance optimization.

### Error ABI

The language exposes named error classes, while the hardware interface mainly
records an error flag and program counter. CML can catch some errors before
emission, but runtime failures need a machine-visible error kind plus location
and, optionally, detail. This can extend the monitor/result protocol without
requiring a new opcode.

### FPGA machine IR

The FPGA emitter currently hardcodes scratch registers in each compilation
routine. This has already produced real clobbering bugs. Before bignums, GC
calls, or substantially more complex expressions, CML needs a lower FPGA
machine IR with virtual registers, liveness, and allocation before assembly
emission.

## Recommended order

1. Keep language semantics and fixtures authoritative in my-lisp.
2. Complete CML's typed lowering diagnostics and explicit capability matrices.
3. Make the CML/fpga-lisp execution ABI machine-readable and jointly tested.
4. Implement first-class FPGA primitives using the existing primitive tag.
5. Add machine-visible named error kinds.
6. Introduce FPGA machine IR and register allocation in CML.
7. Ratify bignum/rational representation through my-lisp fixtures.
8. Implement boxed numeric values and arithmetic on FPGA.
9. Add tracing garbage collection.
10. Claim full compiled-language conformance only from the differential matrix.

## Repository responsibility

- `my-lisp`: owns observable semantics, contract versions, canonical fixtures,
  and representation-level semantic invariants.
- `cml`: owns frontend admission, semantic IR, target validation, partitioning,
  machine IR, ABI use, code generation, and differential conformance.
- `fpga-lisp`: owns ISA encoding, tagged-word and heap representation, runtime
  hardware behavior, monitor/error protocol, synthesis constraints, and RTL
  evidence.

The three repositories already form the right system. The highest-leverage
next step is not a new backend but a stronger formal seam between CML and
fpga-lisp: shared ABI, capability declarations, error protocol, and value
representation contracts.

## GPU preparation status

CML now owns an analysis-only compute contract (`compute-contract.my`, version
0.1) and recognizes `map`/`reduce` execution shapes without changing my-lisp.
GPU admission remains fail-closed. The current my-lisp `Vector` is
heterogeneous and mutable through `vector-set!`; it is therefore not silently
treated as a typed GPU buffer. Any immutable/typed contiguous representation
is a future my-lisp contract decision, after which CML may refine storage and
numeric facts and select a backend without changing program results.

---

# my-lisp, CML та fpga-lisp: єдина семантична система (Ukrainian)

Статус: архітектурний аналіз, 2026-08-24. Цей документ фіксує взаємозв'язок,
що спостерігається у трьох живих репозиторіях; машинно-читабельні контракти
залишаються більш авторитетними, ніж цей текст.

## Ролі

```text
my-lisp    визначає, що означає програма
CML        знижує це значення до виконуваних форм
fpga-lisp  фізично виконує одну з цих форм
```

`my-lisp` є семантичним авторитетом. CML — це точка інтеграції, а не друге
визначення Lisp. `fpga-lisp` є одночасно і цільовою ISA для AOT-програм CML,
і незалежним апаратним обчислювачем. Ці дві FPGA-ролі мають залишатися різними:

```text
CML -> асемблер -> fpga-lisp ISA     скомпільоване виконання
Lisp дані -> апаратний eval/apply    незалежна семантична реалізація
```

Перший шлях тестує компілятор. Другий тестує універсальність мови та запобігає
ситуації, коли CML та FPGA-backend погоджуються щодо однакової хибної
інтерпретації.

## Поточні межі контрактів

| Компонент | Оголошена межа | Спостережуваний стан |
|---|---|---|
| my-lisp | мовний контракт 3.0 | семантичне джерело істини |
| CML | мова 2.0, частковий допуск можливостей з 2.1 | спільний IR, C та FPGA генератори |
| fpga-lisp | ISA 1.1 | 32-бітна тегована Lisp-машина з розширеним фреймом регістрів-входів |
| CML -> FPGA | мова 2.0 / ISA 1.1 | реальний асемблер, транспорт і апаратний шлях |

Шлях FPGA вже підтримує теговані значення, купу (heap) cons-комірок на 4096
елементів, образ програми на 4096 слів, шістнадцять регістрів, примітиви Маккарті,
ADD/SUB, керування потоком, виклики, замикання, лексичні середовища,
варіативні аргументи, рекурсивні визначення, а також скомпільовані приклади,
включно з `reverse`, `append` та структурним `equal?`.

## Цільова архітектура

```text
                     my-lisp
              версіонований мовний контракт
                         |
                 канонічні фікстури
                         |
                         v
                  CML семантичний IR
                         |
             аналіз + представлення
              +----------+----------+
              |                     |
          C середовище         FPGA машина IR
                                    |
                           розподіл регістрів
                                    |
                         спільний ABI виконання
                                    |
                              fpga-lisp ISA
                                    |
                                  RTL
```

Робота з потоками даних (dataflow) для GPU та FPGA знаходиться поряд із цим
шляхом, а не всередині семантики мови:

```text
CML семантичний IR
  +-- загальне зниження Lisp -> C runtime / fpga-lisp машина
  `-- compute/dataflow IR    -> GPU / спеціалізований FPGA-конвеєр
```

Отже, `fpga-lisp` не є просто "ще одним бекендом на кшталт GPU". Він може виконувати
загальний Lisp через свою ISA машини, тоді як майбутній FPGA dataflow backend
зможе спеціалізувати виключно stream або pipeline регіони без обчислювача.

## Важливі шви для формалізації

### Першокласні вбудовані функції (First-class builtins)

Мовний контракт 2.1 робить вбудовані функції звичайними значеннями, які можна
викликати, передавати та лексично перекривати (shadow). CML C-backend має цю
можливість, тоді як FPGA-генератор все ще знижує виклики примітивів безпосередньо.
Апаратне забезпечення вже має `TAG_PRIMITIVE`; наступний дизайн повинен
впровадити значення примітивів у середовище і змусити як апаратний обчислювач,
так і згенеровані CML виклики використовувати те саме правило пошуку/застосування.

### Execution ABI

CML наразі документує діючу конвенцію викликів FPGA: R0 несе повний список
аргументів, R4 — середовище, R11 — програмний стек на базі cons, R14 — адресу 
повернення, а R15 — результат. Це контракт між двома репозиторіями, а не просто
приватна нотатка реалізації. Вона має стати машинно-читабельним спільним ABI,
який перевіряється і CML, і fpga-lisp.

### Числове представлення

my-lisp гарантує точні цілі числа довільної точності (bignums) та точні
нормалізовані раціональні числа. FPGA наразі має fixnum-навантаження, а CML
додатково обмежує літерали діапазоном безпосереднього (immediate) завантаження
`LOADI`. Запропоновані упаковані (boxed) bignum зі знаком-модулем та
представлення чисельника/знаменника для раціональних чисел є концептуально
правильними, але вони мають бути затверджені через фікстури до реалізації в RTL.
Чернетка представлення також вказує, що залишилося лише два слоти тегів, хоча
ISA 1.0 використовує шість із шістнадцяти значень тегів; це твердження потребує
виправлення або явного опису інших зарезервованих тегів.

Безпечна послідовність така:

```text
my-lisp фікстури
-> контракт представлення
-> ревізія тегів ISA
-> RTL конструктори/аксесоари
-> арифметичні підпрограми
-> CML зниження представлення
-> диференційне узгодження (differential conformance)
```

### Купа та збирання сміття (Garbage collection)

Стек R11 сам по собі є cons-списком у тій самій 4096-комірковій купі
bump-алокатора. Таким чином, виклики споживають пам'ять купи навіть після
логічних `pop`. `SETCDR` також дозволяє створювати цикли через рекурсивні
середовища замикань, тому підрахунок посилань (reference counting) недостатній.
Трасувальний збирач сміття (tracing GC) є важливою віхою коректності виконання
для більших скомпільованих програм, а не просто оптимізацією продуктивності.

### Error ABI

Мова відкриває іменовані класи помилок, тоді як апаратний інтерфейс здебільшого
фіксує прапорець помилки та лічильник програм. CML може відловлювати деякі
помилки до генерації, але runtime-відмови потребують машинно-видимого типу 
помилки разом із локацією та, опціонально, деталями. Це може розширити протокол
монітора/результату без введення нового коду операції (opcode).

### FPGA машина IR

FPGA-генератор наразі жорстко закодовує тимчасові регістри (scratch registers)
у кожній процедурі компіляції. Це вже призвело до реальних помилок затирання.
Перед впровадженням bignums, викликів GC або суттєво складніших виразів, CML 
потребує нижчого FPGA машинного IR із віртуальними регістрами, аналізом життя
(liveness) та розподілом перед генерацією асемблера.

## Рекомендований порядок

1. Зберігати семантику мови та фікстури авторитетними у my-lisp.
2. Завершити діагностику типізованого зниження CML та явні матриці можливостей.
3. Зробити Execution ABI між CML/fpga-lisp машинно-читабельним і спільно тестованим.
4. Реалізувати першокласні FPGA-примітиви, використовуючи існуючий тег примітивів.
5. Додати машинно-видимі іменовані типи помилок.
6. Впровадити FPGA машинні IR та розподіл регістрів у CML.
7. Ратифікувати представлення bignum/rational через фікстури my-lisp.
8. Реалізувати упаковані (boxed) числові значення та арифметику на FPGA.
9. Додати tracing збирання сміття (GC).
10. Заявляти про повну відповідність скомпільованій мові лише на основі диференційної матриці.

## Відповідальність репозиторіїв

- `my-lisp`: володіє видимою семантикою, версіями контрактів, канонічними фікстурами
  та семантичними інваріантами рівня представлення.
- `cml`: володіє фронтенд-допуском, семантичним IR, валідацією цілей, партиціонуванням,
  машинним IR, використанням ABI, генерацією коду та диференційним узгодженням.
- `fpga-lisp`: володіє кодуванням ISA, представленням тегованого слова та купи,
  поведінкою апаратного середовища, протоколом монітора/помилок, обмеженнями синтезу
  та RTL доказами.

Ці три репозиторії вже утворюють правильну систему. Найефективнішим наступним
кроком є не новий бекенд, а міцніший формальний шов між CML та fpga-lisp:
спільний ABI, декларації можливостей, протокол помилок та контракти представлення
значень.

## Статус підготовки до GPU

CML тепер має обчислювальний контракт (`compute-contract.my`, версія 0.1) лише
для аналізу і розпізнає шаблони виконання `map`/`reduce` без змін у my-lisp.
GPU допуск (admission) залишається fail-closed. Поточний my-lisp `Vector` є
гетерогенним і змінним через `vector-set!`; тому він не розглядається мовчки
як типізований GPU-буфер. Будь-яке незмінне/типізоване безперервне представлення —
це майбутнє рішення мовного контракту my-lisp, після якого CML зможе уточнювати
дані про зберігання й числа та обирати бекенд без зміни результатів програми.
