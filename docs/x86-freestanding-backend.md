# x86_64 freestanding backend — first slice

`src/x86_freestanding.rs` is a deterministic GNU assembly consumer of the
shared admitted `Ir`. It imports its numeric value representation from the
`wsm-os-target` crate pinned in `Cargo.toml`; it does not copy Rust
`my_lisp::Value`, NaN-boxing, or host pointer layouts.

The initial supported surface is intentionally bounded:

- integer, `()` and `t` immediates;
- quoted integers, symbols, proper lists and dotted lists;
- quoted strings are rejected (the target ABI has no distinct string value);
- `cons`, `car`, `cdr`, `eq` and `atom` through the versioned `wsm_*` ABI;
- `cond` boolean branching (checking strict `()` identity);
- checked fixnum arithmetic (`+`, `-`) yielding opaque boundary errors on overflow;
- loop-optimized self-tail-calls (constant stack frame footprint).

Every other IR node fails during a complete preflight pass before the output
buffer exists. There is no libc, syscall, filesystem, C-backend fallback, or
claim of full my-lisp 3.0 support.

The emitted entry point follows the target contract:

```text
Value wsm_entry(RuntimeContext *context)
```

It preserves the opaque context in callee-saved `r12`, keeps the stack aligned
before runtime calls, preserves the SysV AMD64 callee-saved register, disables
the executable stack note, and returns the final value word in `rax`.

`tests/x86_freestanding_test.rs` assembles generated `.s` with a real host
assembler and inspects the resulting object with `nm -u`. Undefined symbols
must be a subset of the target contract's `wsm_*` runtime imports. Runtime
behavior and QEMU boot parity remain separate later milestones.

---

# x86_64 freestanding backend — перший зріз (Ukrainian)

`src/x86_freestanding.rs` є детермінованим генератором GNU assembly з
узгодженого (admitted) `Ir`. Він імпортує своє числове представлення значень
через крейт `wsm-os-target`, закріплений (pinned) у `Cargo.toml`; він не копіює
Rust `my_lisp::Value`, NaN-boxing, чи макети вказівників хоста.

Початкова підтримувана поверхня навмисно обмежена:

- integer, `()` та `t` літерали (immediates);
- quoted integers, символи, звичайні (proper) та точкові (dotted) списки;
- quoted strings відхиляються: цільовий ABI ще не має окремого string value;
- `cons`, `car`, `cdr`, `eq` та `atom` через версіонований ABI `wsm_*`;
- логічні розгалуження `cond` (перевірка строгої ідентичності `()`);
- безпечна (checked) fixnum арифметика (`+`, `-`), яка при переповненні
  повертає помилку виходу за межі;
- само-рекурсивні хвостові виклики, оптимізовані в цикли (сталий розмір фрейму стеку).

Будь-який інший IR-вузол зазнає невдачі (fails) під час повного етапу
попередньої перевірки (preflight) ще до створення вихідного буфера. Тут
немає libc, системних викликів (syscalls), файлової системи, fallbacks на
C-backend, чи претензій на повну підтримку `my-lisp 3.0`.

Згенерована точка входу відповідає цільовому контракту:

```text
Value wsm_entry(RuntimeContext *context)
```

Він зберігає непрозорий (opaque) контекст у callee-saved регістрі `r12`,
підтримує вирівнювання стеку перед викликами runtime, зберігає callee-saved
регістри згідно із SysV AMD64, вимикає прапорець виконуваного стеку (executable
stack note) і повертає кінцеве значення у регістрі `rax`.

`tests/x86_freestanding_test.rs` асемблює згенерований `.s` за допомогою
реального асемблера хоста та інспектує отриманий об'єкт через `nm -u`. 
Невизначені (undefined) символи повинні бути лише підмножиною імпортів
`wsm_*` з цільового контракту. Перевірка поведінки під час виконання та
завантаження в QEMU (QEMU boot parity) є окремими, пізнішими віхами.

FS boundary: hosted WSM filesystem names such as `"notes/today"` remain
strings and are not silently re-encoded as image-local symbols by this
backend. A future freestanding FS image needs a separately ratified string
or name-reference representation.

Межа FS: імена hosted WSM filesystem, наприклад `"notes/today"`, залишаються
рядками й не перекодовуються мовчки в image-local symbols. Майбутній
freestanding FS image потребує окремо ратифікованого представлення рядка або
посилання на ім'я.
