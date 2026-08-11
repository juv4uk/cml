# Testing · Тестування · Testen

[English](#english) · [Українська](#українська) · [Deutsch](#deutsch)

## English

There is a single test: [`tests/conformance_test.rs`](../tests/conformance_test.rs). It is a *blind* adapter — one fixed `parse → macro-expand → compile → assemble → simulate → decode → compare` pipeline that runs against every fixture unmodified, rather than a bespoke check per fixture. This mirrors fpga-lisp's [First Blind Fixture](https://github.com/juv4uk/fpga-lisp) discipline: no fixture-specific branches are allowed inside the adapter itself.

Fixtures are not owned by this repo — they live in the sibling [`my-lisp`](https://github.com/juv4uk/my-lisp) repository at `tests/fixtures/conformance.my`, one alist per line: `((expr . "(quote radio)") (expected . "radio") (tier . 1))`, or for fixtures that must fail statically or at runtime, `((expr . "...") (error . "Arity") (tier . 1))`.

Pipeline, in order:
1. **Filter** — only lines tagged `(tier . 1)` run today; `3.0`-bearing fixtures are skipped (fpga-lisp's ISA has no inexact/rational tag yet — [plan item 25](https://github.com/juv4uk/fpga-lisp/blob/master/docs/lisp-machine-plan.md), not started).
2. **Parse** — [`cml::parser::parse`](../src/parser.rs).
3. **Macro-expand** — [`cml::macros::MacroExpander`](../src/macros.rs) runs before anything else touches the AST; `defmacro` never reaches the compiler.
4. **Static-error check** — [`static_error`](../tests/conformance_test.rs) classifies arity/unknown-symbol failures the compiler's front end can see without running anything. If the fixture expects one of these, the test asserts the match and stops there.
5. **Compile** — [`cml::compiler::Compiler`](../src/compiler.rs) lowers the expanded AST to fpga-lisp assembly text.
6. **Assemble** — shells out to `python3 ../fpga-lisp/assembler.py`, producing a `.bin`.
7. **Simulate** — copies the `.bin` into fpga-lisp's `fpga/sim/` and runs `vvp tb_cml_e2e.vvp +bin_file=...` (requires [Icarus Verilog](http://iverilog.icarus.com/)); this testbench is fpga-lisp's general-purpose E2E harness, not tied to a single milestone.
8. **Decode** — reads `RESULT_TAG`/`RESULT_VAL`/`RESULT_ERROR`/`HEAP:...` lines from the simulator's stdout and canonically decodes atoms, fixnums, proper lists, and dotted lists from the heap.
9. **Compare** — asserts the decoded result (or `RESULT_ERROR`) against the fixture's `expected`/`error` field.

Run it locally:

```bash
cargo test --test conformance_test
```

This requires, checked out as siblings of this repo (matching the `.github/workflows/ci.yml` layout):
- `../my-lisp` — provides `tests/fixtures/conformance.my`
- `../fpga-lisp` — provides `assembler.py` and the compiled `tb_cml_e2e.vvp` testbench
- `python3` and `iverilog` on `PATH`

CI ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)) checks out both siblings fresh on every push/PR to `master`, so a missing local toolchain (no `iverilog`, no sibling checkouts) is not a blocker for landing a change — it only blocks *local* verification. See [`ecosystem-status.md`](../ecosystem-status.md) for the running log of what has and hasn't been machine-verified this way.

## Українська

Є один тест: [`tests/conformance_test.rs`](../tests/conformance_test.rs). Це *сліпий* адаптер — один незмінний конвеєр `parse → macro-expand → compile → assemble → simulate → decode → compare`, що прогонить кожну фікстуру без модифікацій, а не окрема перевірка під кожну фікстуру. Це дзеркалить дисципліну fpga-lisp "First Blind Fixture": усередині самого адаптера не допускаються гілки під конкретну фікстуру.

Фікстури не належать цьому репозиторію — вони живуть у сусідньому [`my-lisp`](https://github.com/juv4uk/my-lisp) за шляхом `tests/fixtures/conformance.my`, по одному alist на рядок: `((expr . "(quote radio)") (expected . "radio") (tier . 1))`, або для фікстур, що мають статично чи в рантаймі впасти: `((expr . "...") (error . "Arity") (tier . 1))`.

Конвеєр по кроках:
1. **Фільтр** — сьогодні прогоняються лише рядки з тегом `(tier . 1)`; фікстури з `3.0` пропускаються (в ISA fpga-lisp ще немає inexact/rational тегу — [пункт 25 плану](https://github.com/juv4uk/fpga-lisp/blob/master/docs/lisp-machine-plan.md), ще не почато).
2. **Parse** — [`cml::parser::parse`](../src/parser.rs).
3. **Macro-expand** — [`cml::macros::MacroExpander`](../src/macros.rs) виконується перш, ніж будь-що інше торкнеться AST; `defmacro` ніколи не доходить до компілятора.
4. **Перевірка статичних помилок** — [`static_error`](../tests/conformance_test.rs) класифікує помилки арності/невідомого символу, які front-end бачить без виконання. Якщо фікстура очікує саме це — тест звіряє й зупиняється тут.
5. **Compile** — [`cml::compiler::Compiler`](../src/compiler.rs) знижує розгорнутий AST в текст асемблера fpga-lisp.
6. **Assemble** — викликає `python3 ../fpga-lisp/assembler.py`, отримуючи `.bin`.
7. **Simulate** — копіює `.bin` у `fpga/sim/` fpga-lisp і запускає `vvp tb_cml_e2e.vvp +bin_file=...` (потрібен [Icarus Verilog](http://iverilog.icarus.com/)); цей testbench — загальний E2E-стенд fpga-lisp, не прив'язаний до конкретного milestone.
8. **Decode** — читає рядки `RESULT_TAG`/`RESULT_VAL`/`RESULT_ERROR`/`HEAP:...` зі stdout симулятора й канонічно декодує atoms, fixnums, proper lists і dotted lists із heap.
9. **Compare** — звіряє декодований результат (чи `RESULT_ERROR`) з полем `expected`/`error` фікстури.

Запуск локально:

```bash
cargo test --test conformance_test
```

Це вимагає, як сусідні репозиторії поряд із цим (той самий layout, що й у `.github/workflows/ci.yml`):
- `../my-lisp` — надає `tests/fixtures/conformance.my`
- `../fpga-lisp` — надає `assembler.py` і зібраний testbench `tb_cml_e2e.vvp`
- `python3` і `iverilog` у `PATH`

CI ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)) заново чекаутить обидва сусідні репо при кожному push/PR у `master`, тож відсутність локального інструментарію (немає `iverilog`, немає сусідніх чекаутів) не блокує зміну — блокує лише *локальну* перевірку. Див. [`ecosystem-status.md`](../ecosystem-status.md) — журнал того, що й коли було machine-verified у такий спосіб.

## Deutsch

Es gibt einen Test: [`tests/conformance_test.rs`](../tests/conformance_test.rs). Es ist ein *blinder* Adapter — eine feste Pipeline `parse → macro-expand → compile → assemble → simulate → decode → compare`, die für jede Fixture unverändert läuft, statt einer maßgeschneiderten Prüfung pro Fixture. Das spiegelt die "First Blind Fixture"-Disziplin von fpga-lisp: Innerhalb des Adapters selbst sind keine Fixture-spezifischen Verzweigungen erlaubt.

Fixtures gehören nicht zu diesem Repository — sie liegen im benachbarten [`my-lisp`](https://github.com/juv4uk/my-lisp)-Repository unter `tests/fixtures/conformance.my`, eine Alist pro Zeile: `((expr . "(quote radio)") (expected . "radio") (tier . 1))`, oder für Fixtures, die statisch oder zur Laufzeit fehlschlagen müssen: `((expr . "...") (error . "Arity") (tier . 1))`.

Pipeline, der Reihe nach:
1. **Filter** — heute laufen nur Zeilen mit Tag `(tier . 1)`; Fixtures mit `3.0` werden übersprungen (fpga-lisps ISA hat noch kein inexact/rational-Tag — [Planpunkt 25](https://github.com/juv4uk/fpga-lisp/blob/master/docs/lisp-machine-plan.md), noch nicht begonnen).
2. **Parse** — [`cml::parser::parse`](../src/parser.rs).
3. **Macro-expand** — [`cml::macros::MacroExpander`](../src/macros.rs) läuft, bevor irgendetwas anderes den AST berührt; `defmacro` erreicht den Compiler nie.
4. **Statische Fehlerprüfung** — [`static_error`](../tests/conformance_test.rs) klassifiziert Stelligkeits-/Unbekanntes-Symbol-Fehler, die das Frontend ohne Ausführung erkennt. Erwartet die Fixture genau das, prüft der Test hier und stoppt.
5. **Compile** — [`cml::compiler::Compiler`](../src/compiler.rs) senkt den expandierten AST zu fpga-lisp-Assemblertext ab.
6. **Assemble** — ruft `python3 ../fpga-lisp/assembler.py` auf und erzeugt eine `.bin`.
7. **Simulate** — kopiert die `.bin` in fpga-lisps `fpga/sim/` und führt `vvp tb_cml_e2e.vvp +bin_file=...` aus (benötigt [Icarus Verilog](http://iverilog.icarus.com/)); dieser Testbench ist fpga-lisps universeller E2E-Prüfstand, nicht an einen einzelnen Meilenstein gebunden.
8. **Decode** — liest `RESULT_TAG`/`RESULT_VAL`/`RESULT_ERROR`/`HEAP:...`-Zeilen aus der stdout des Simulators und dekodiert Atome, Fixnums, echte Listen und Dotted Lists kanonisch aus dem Heap.
9. **Compare** — vergleicht das dekodierte Ergebnis (oder `RESULT_ERROR`) mit dem `expected`/`error`-Feld der Fixture.

Lokal ausführen:

```bash
cargo test --test conformance_test
```

Dies erfordert, als Geschwister dieses Repos ausgecheckt (gleiches Layout wie `.github/workflows/ci.yml`):
- `../my-lisp` — liefert `tests/fixtures/conformance.my`
- `../fpga-lisp` — liefert `assembler.py` und den gebauten Testbench `tb_cml_e2e.vvp`
- `python3` und `iverilog` im `PATH`

CI ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)) checkt beide Geschwister-Repos bei jedem Push/PR nach `master` frisch aus — ein fehlendes lokales Toolchain (kein `iverilog`, keine Geschwister-Checkouts) blockiert also keine Änderung, nur die *lokale* Verifikation. Siehe [`ecosystem-status.md`](../ecosystem-status.md) für das laufende Protokoll, was auf diese Weise wann machine-verified wurde.
