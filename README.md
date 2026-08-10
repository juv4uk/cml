# cml

**Ahead-of-Time (AOT) Compiler for my-lisp targeting fpga-lisp architecture**

[English](#english) · [Українська](#українська) · [Deutsch](#deutsch)

## English

`cml` is an Ahead-of-Time (AOT) compiler that translates `my-lisp` source code directly into `fpga-lisp` assembly. Rather than running a full Lisp evaluator loop (`eval`/`apply`) on the hardware at runtime, `cml` performs the compilation on a host machine, generating fixed machine code for the FPGA.

This approach bypasses the runtime interpretation overhead. The long-term goal is to execute complex programs like `unify.my` and `reason.my` (from the "Advice Taker" priority) significantly faster directly on the `fpga-lisp` hardware.

The compiler handles:
- Variables (via compile-time environment lookup injection)
- `cond` (branching logic compiled to `JF`/`RET`)
- `lambda` (closures that self-bind arguments to an environment)
- Standard primitives (`cons`, `car`, `cdr`, `eq`, `atom`)
- Quoted lists (`'(a b c)`)
- Call Stack (`R11` software stack for environment and link preservation)

### Current Limitations
- Maximum of 3 arguments for generic function calls.
- Dotted lists are not supported.
- Strings are not supported.

[View Test Results](test_results.md)

### Related Repositories
- [fpga-lisp](https://github.com/juv4uk/fpga-lisp): The hardware architecture and assembler.
- [my-lisp](https://github.com/juv4uk/my-lisp): The Lisp dialect.
- [cml](https://github.com/juv4uk/cml): This compiler.

### Build and Run

```bash
cargo build
cargo run -- path/to/source.my
```

## Українська

`cml` — це Ahead-of-Time (AOT) компілятор, який перетворює сирцевий код `my-lisp` безпосередньо в асемблер `fpga-lisp`. Замість того, щоб запускати повний цикл обчислення Lisp (`eval`/`apply`) на апаратному забезпеченні під час виконання, `cml` виконує компіляцію на хост-комп'ютері, генеруючи фіксований машинний код для FPGA.

Цей підхід дозволяє уникнути накладних витрат на інтерпретацію під час виконання. Довгострокова мета полягає в тому, щоб складні програми, такі як `unify.my` та `reason.my` (пріоритет "Advice Taker"), виконувалися значно швидше безпосередньо на апаратурі `fpga-lisp`.

Компілятор підтримує:
- Змінні (через ін'єкцію пошуку в середовищі на етапі компіляції)
- `cond` (логіка розгалуження, скомпільована в `JF`/`RET`)
- `lambda` (замикання, які самостійно прив'язують аргументи до середовища)
- Стандартні примітиви (`cons`, `car`, `cdr`, `eq`, `atom`)
- Списки з квотуванням (Quoted lists, `'(a b c)`)
- Стек викликів (програмний стек `R11` для збереження середовища та адреси повернення)

### Поточні обмеження
- Максимум 3 аргументи для викликів узагальнених функцій.
- Dotted lists (крапкові списки) не підтримуються.
- Рядки (Strings) не підтримуються.

[Переглянути результати тестів](test_results.md)

### Пов'язані репозиторії
- [fpga-lisp](https://github.com/juv4uk/fpga-lisp): Апаратна архітектура та асемблер.
- [my-lisp](https://github.com/juv4uk/my-lisp): Діалект Lisp.
- [cml](https://github.com/juv4uk/cml): Цей компілятор.

### Збірка та Запуск

```bash
cargo build
cargo run -- path/to/source.my
```

## Deutsch

`cml` ist ein Ahead-of-Time (AOT)-Compiler, der `my-lisp`-Quellcode direkt in `fpga-lisp`-Assembler übersetzt. Anstatt zur Laufzeit eine vollständige Lisp-Auswertungsschleife (`eval`/`apply`) auf der Hardware auszuführen, führt `cml` die Kompilierung auf einem Host-Computer durch und erzeugt festen Maschinencode für das FPGA.

Dieser Ansatz umgeht den Overhead der Laufzeitinterpretation. Das langfristige Ziel ist es, komplexe Programme wie `unify.my` und `reason.my` (aus der "Advice Taker"-Priorität) deutlich schneller direkt auf der `fpga-lisp`-Hardware auszuführen.

Der Compiler verarbeitet:
- Variablen (über beim Kompilieren injiziertes Umgebungs-Lookup)
- `cond` (Verzweigungslogik, kompiliert zu `JF`/`RET`)
- `lambda` (Closures, die ihre Argumente selbst an eine Umgebung binden)
- Standardprimitiven (`cons`, `car`, `cdr`, `eq`, `atom`)
- Zitierte Listen (`'(a b c)`)
- Aufrufstapel (`R11` Software-Stack für Umgebungs- und Rücksprungadressenspeicherung)

### Aktuelle Einschränkungen
- Maximal 3 Argumente für generische Funktionsaufrufe.
- Dotted Lists werden nicht unterstützt.
- Strings werden nicht unterstützt.

[Testergebnisse anzeigen](test_results.md)

### Verwandte Repositories
- [fpga-lisp](https://github.com/juv4uk/fpga-lisp): Die Hardwarearchitektur und Assembler.
- [my-lisp](https://github.com/juv4uk/my-lisp): Der Lisp-Dialekt.
- [cml](https://github.com/juv4uk/cml): Dieser Compiler.

### Erstellen und Ausführen

```bash
cargo build
cargo run -- path/to/source.my
```
