# cml

**Ahead-of-Time (AOT) Compiler for my-lisp targeting fpga-lisp architecture**

[English](#english) · [Українська](#українська) · [Deutsch](#deutsch)

## English

`cml` is an Ahead-of-Time (AOT) compiler that translates `my-lisp` source code directly into `fpga-lisp` assembly. Rather than running a full Lisp evaluator loop (`eval`/`apply`) on the hardware at runtime, `cml` performs the compilation on a host machine, generating fixed machine code for the FPGA.

This approach bypasses the runtime interpretation overhead and allows complex programs like `unify.my` and `reason.my` (from the "Advice Taker" priority) to execute significantly faster directly on the `fpga-lisp` hardware.

The compiler handles:
- Variables (via compile-time environment lookup injection)
- `cond` (branching logic compiled to `JF`/`JMP`)
- `lambda` (closures that self-bind arguments to an environment)
- Standard primitives (`cons`, `car`, `cdr`, `eq`, `atom`)

[View Test Results](test_results.md)

### Build and Run

```bash
cargo build
cargo run -- path/to/source.my
```

## Українська

`cml` — це Ahead-of-Time (AOT) компілятор, який перетворює сирцевий код `my-lisp` безпосередньо в асемблер `fpga-lisp`. Замість того, щоб запускати повний цикл обчислення Lisp (`eval`/`apply`) на апаратному забезпеченні під час виконання, `cml` виконує компіляцію на хост-комп'ютері, генеруючи фіксований машинний код для FPGA.

Цей підхід дозволяє уникнути накладних витрат на інтерпретацію під час виконання і дає можливість складним програмам, таким як `unify.my` та `reason.my` (пріоритет "Advice Taker"), виконуватися значно швидше безпосередньо на апаратурі `fpga-lisp`.

Компілятор підтримує:
- Змінні (через ін'єкцію пошуку в середовищі на етапі компіляції)
- `cond` (логіка розгалуження, скомпільована в `JF`/`JMP`)
- `lambda` (замикання, які самостійно прив'язують аргументи до середовища)
- Стандартні примітиви (`cons`, `car`, `cdr`, `eq`, `atom`)

[Переглянути результати тестів](test_results.md)

### Збірка та Запуск

```bash
cargo build
cargo run -- path/to/source.my
```

## Deutsch

`cml` ist ein Ahead-of-Time (AOT)-Compiler, der `my-lisp`-Quellcode direkt in `fpga-lisp`-Assembler übersetzt. Anstatt zur Laufzeit eine vollständige Lisp-Auswertungsschleife (`eval`/`apply`) auf der Hardware auszuführen, führt `cml` die Kompilierung auf einem Host-Computer durch und erzeugt festen Maschinencode für das FPGA.

Dieser Ansatz umgeht den Overhead der Laufzeitinterpretation und ermöglicht es, komplexe Programme wie `unify.my` und `reason.my` (aus der "Advice Taker"-Priorität) deutlich schneller direkt auf der `fpga-lisp`-Hardware auszuführen.

Der Compiler verarbeitet:
- Variablen (über beim Kompilieren injiziertes Umgebungs-Lookup)
- `cond` (Verzweigungslogik, kompiliert zu `JF`/`JMP`)
- `lambda` (Closures, die ihre Argumente selbst an eine Umgebung binden)
- Standardprimitiven (`cons`, `car`, `cdr`, `eq`, `atom`)

[Testergebnisse anzeigen](test_results.md)

### Erstellen und Ausführen

```bash
cargo build
cargo run -- path/to/source.my
```
