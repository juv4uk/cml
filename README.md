# cml

**Heterogeneous AOT compiler middle-end for my-lisp**

[English](#english) · [Українська](#українська) · [Deutsch](#deutsch)

## English

`cml` is an Ahead-of-Time (AOT) compiler middle-end for `my-lisp`. It lowers one semantic IR toward CPU/C, GPU compute backends, and `fpga-lisp` execution. Backend capabilities remain explicit and fail-closed.

This approach separates language semantics from physical execution. The current graph has live CPU, CUDA, and FPGA paths, including a host-staged typed-buffer-to-FPGA-register edge; direct GPU-to-FPGA transfer is not claimed.

The compiler handles:
- Variables (via compile-time environment lookup injection)
- `cond` (branching logic compiled to `JF`/`RET`)
- `lambda` (closures that self-bind arguments to an environment)
- `let` lowered as an immediately invoked `lambda`, without a new FPGA primitive
- Standard primitives (`cons`, `car`, `cdr`, `eq`, `atom`)
- Quoted lists (`'(a b c)`)
- Call Stack (`R11` software stack for environment and link preservation)

### Current Limitations
- Generic calls bind at most 8 arguments; additional arguments are not yet rejected explicitly.
- The conformance runner canonically decodes atoms, fixnums, proper lists, and dotted lists from the FPGA heap; unsupported language forms are still skipped explicitly.
- Tier-1 error fixtures are observable too: the compiler classifies static arity/unknown-symbol failures, while FPGA execution reports runtime type failures through a machine-readable result channel.
- Source strings currently lower to target symbols; fpga-lisp has no distinct runtime string tag yet.
- Inexact numbers and exact rationals are not supported by the target representation.

[`compatibility.my`](compatibility.my) records the exact my-lisp language contract, fpga-lisp ISA contract, tested SHAs, supported surface, and known gaps for this compiler revision.

[View Test Results](test_results.md) · [Testing](docs/testing.md)

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

`cml` — це heterogeneous middle-end і Ahead-of-Time (AOT) компілятор для `my-lisp`. Він знижує єдиний semantic IR до CPU/C, GPU compute backends та виконання `fpga-lisp`, залишаючи можливості backend'ів явними й fail-closed.

Цей підхід відділяє семантику мови від фізичного виконання. Поточний graph має live CPU, CUDA і FPGA шляхи, включно з host-staged typed-buffer→FPGA-register edge; прямий GPU→FPGA transfer не заявляється.

Компілятор підтримує:
- Змінні (через ін'єкцію пошуку в середовищі на етапі компіляції)
- `cond` (логіка розгалуження, скомпільована в `JF`/`RET`)
- `lambda` (замикання, які самостійно прив'язують аргументи до середовища)
- `let`, знижений до негайно викликаної `lambda` без нової FPGA-примітиви
- Стандартні примітиви (`cons`, `car`, `cdr`, `eq`, `atom`)
- Списки з квотуванням (Quoted lists, `'(a b c)`)
- Стек викликів (програмний стек `R11` для збереження середовища та адреси повернення)

### Поточні обмеження
- Generic calls зв'язують щонайбільше 8 аргументів; зайві аргументи ще не відхиляються явно.
- Conformance runner канонічно декодує atoms, fixnums, proper lists і dotted lists із FPGA heap; непідтримані мовні форми досі пропускаються явно.
- Tier-1 error fixtures теж спостережувані: компілятор класифікує статичні помилки арності/невідомого символу, а FPGA повертає runtime-помилки типу через машинозчитуваний канал результату.
- Сирцеві strings поки знижуються до target symbols; fpga-lisp ще не має окремого runtime string tag.
- Inexact numbers і точні rationals не підтримуються цільовим представленням.

[`compatibility.my`](compatibility.my) фіксує точний language contract my-lisp, ISA contract fpga-lisp, перевірені SHA, підтриману поверхню й відомі прогалини цієї ревізії компілятора.

[Переглянути результати тестів](test_results.md) · [Тестування](docs/testing.md)

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

`cml` ist ein heterogenes AOT-Compiler-Middle-End für `my-lisp`. Es senkt eine gemeinsame semantische IR zu CPU/C-, GPU-Compute-Backends und `fpga-lisp` ab; Backend-Fähigkeiten bleiben explizit und fail-closed.

Dieser Ansatz trennt Sprachsemantik von physischer Ausführung. Der aktuelle Graph hat live CPU-, CUDA- und FPGA-Pfade einschließlich eines host-gestagten Typed-Buffer→FPGA-Register-Pfads; direkter GPU→FPGA-Transfer wird nicht behauptet.

Der Compiler verarbeitet:
- Variablen (über beim Kompilieren injiziertes Umgebungs-Lookup)
- `cond` (Verzweigungslogik, kompiliert zu `JF`/`RET`)
- `lambda` (Closures, die ihre Argumente selbst an eine Umgebung binden)
- `let`, abgesenkt zu einer sofort aufgerufenen `lambda` ohne neue FPGA-Primitive
- Standardprimitiven (`cons`, `car`, `cdr`, `eq`, `atom`)
- Zitierte Listen (`'(a b c)`)
- Aufrufstapel (`R11` Software-Stack für Umgebungs- und Rücksprungadressenspeicherung)

### Aktuelle Einschränkungen
- Generische Aufrufe binden höchstens 8 Argumente; zusätzliche Argumente werden noch nicht explizit abgelehnt.
- Der Konformitätsrunner dekodiert Atome, Fixnums, echte Listen und Dotted Lists aus dem FPGA-Heap kanonisch; nicht unterstützte Sprachformen werden weiterhin explizit übersprungen.
- Auch Tier-1-Fehler-Fixtures sind beobachtbar: Der Compiler klassifiziert statische Stelligkeits- und Unbekanntes-Symbol-Fehler, während das FPGA Laufzeit-Typfehler über einen maschinenlesbaren Ergebniskanal meldet.
- Quell-Strings werden derzeit zu Zielsymbolen abgesenkt; fpga-lisp besitzt noch kein eigenes Laufzeit-String-Tag.
- Inexakte Zahlen und exakte rationale Zahlen werden von der Zieldarstellung nicht unterstützt.

[`compatibility.my`](compatibility.my) hält den genauen my-lisp-Sprachvertrag, fpga-lisp-ISA-Vertrag, geprüfte SHAs, die unterstützte Oberfläche und bekannte Lücken dieser Compilerrevision fest.

[Testergebnisse anzeigen](test_results.md) · [Testen](docs/testing.md)

### Verwandte Repositories
- [fpga-lisp](https://github.com/juv4uk/fpga-lisp): Die Hardwarearchitektur und Assembler.
- [my-lisp](https://github.com/juv4uk/my-lisp): Der Lisp-Dialekt.
- [cml](https://github.com/juv4uk/cml): Dieser Compiler.

### Erstellen und Ausführen

```bash
cargo build
cargo run -- path/to/source.my
```
