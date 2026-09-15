# Чинна документація та авторитет — CML (Current Documentation Entry Point)

**Починайте звідси.** Цей документ є єдиною канонічною точкою входу до архітектури, контрактів та планів компілятора `cml`.  
У разі будь-яких розбіжностей між документами, на які посилається ця сторінка, та матеріалами з [`docs/archive/`](archive/README.md), безумовний пріоритет має актуальна документація. Матеріали в архіві є суто історичними.

**Start here.** This document is the single authoritative entry point to the architecture, contracts, and active plans of the `cml` compiler. On any conflict between documents linked here and materials under [`docs/archive/`](archive/README.md), current material wins unconditionally.

---

## 1. Порядок авторитету (Authority Hierarchy)

1. **Семантика мови (Language Semantics):**
   Повністю належить `my-lisp` (реєстр `lib/surface/semantic-registry.lisp`, контракти та оракул `127.0.0.1:9999`). CML **ніколи не створює** самостійної мовної семантики й не вигадує власних semantic ID.
2. **Декларація ролі репозиторію ([repo.lisp](../repo.lisp)):**
   * Роль: `compiler-middle-end`
   * Володіє: `ir`, `aot-compilation`, `host-target`, `target-selection`
   * Не володіє: `language-semantics`
3. **Чинні контракти та операції ([contracts/](../contracts/)):**
   * `contracts/cml-operations.lisp` (машинна таблиця операцій, згенерована з `my-lisp`)
   * `contracts/cml.lock` (фіксація ревізій)

---

## 2. Ключові архітектурні документи (Normative Architecture Documents)

### Загальна архітектура компілятора та конвеєр
* [`docs/heterogeneous-backends.md`](heterogeneous-backends.md) — межа бекендів: розділення конвеєра на `AST → IR → target emission`. Спільний middle-end для x86-64, C, CUDA, FPGA.
* [`docs/compiler-plan.md`](compiler-plan.md) — чинний технічний план компілятора.
* [`docs/CANON-DISPATCH-TRANSITION-PLAN-2026-09-11.md`](CANON-DISPATCH-TRANSITION-PLAN-2026-09-11.md) — перехід на диспетчеризацію форм через семантичні ID (Canon 0+7).

### Цільовий ABI та низькорівневий бекенд x86-64
* [`docs/abi.md`](abi.md) — чинний контракт System V AMD64 ABI, виклики рантайму, регістри та формати слів.
* [`docs/x86-freestanding-backend.md`](x86-freestanding-backend.md) — автономний x86-64 бекенд для UEFI та голої машини (Pure Lisp + assembly, Zero-Rust target).
* [`src/machine_inst.rs`](../src/machine_inst.rs) — типізований шар машинних інструкцій (`MachineInst`), принтер GNU асемблера та прямий байтовий енкодер x86-64.
* [`docs/research/cml-machine-inst-architecture-proposal-2026-09-14.md`](research/cml-machine-inst-architecture-proposal-2026-09-14.md) — архітектурне дослідження симетричного кодування ALU, трирівневої моделі операндів та одноетапного розрахунку переходів.

### Гетерогенне обчислення (Heterogeneous Execution Fabric)
* [`docs/HETEROGENEOUS-EXECUTION-FABRIC.md`](HETEROGENEOUS-EXECUTION-FABRIC.md) — граф обчислень (`ComputeAnalysis`, `ExecutionGraph`), розподіл навантаження між CPU, GPU та FPGA.
* [`docs/cuda-runtime.md`](cuda-runtime.md) — підтримка та запуск на NVIDIA CUDA.
* [`docs/intel-runtime.md`](intel-runtime.md) — Intel OpenCL/LevelZero інтеграція.
* [`docs/fpga-job-protocol.md`](fpga-job-protocol.md) — бінарний протокол взаємодії з апаратним процесором FPGA.

### Політика тестування та інструментів
* [`docs/testing.md`](testing.md) — конвенції тестування (конформанс, юніт-тести, регресія).
* [`docs/tooling-language-priority.md`](tooling-language-priority.md) — пріоритети інструментів в екосистемі.
* [`docs/THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) — повідомлення про сторонні ліцензії та компоненти.

---

## 3. Чинні плани та задачі (Active Plans & Tasks)

* **Реєстр задач репозиторію:** [`tasks.lisp`](../tasks.lisp)
* **GitHub Issues:** перевіряти наживо через `gh issue list --repo juv4uk/cml --state open`
* **Дослідницькі гілки:** `docs/research/` та гілки `research/*`

---

## 4. Архів (Archive)

Усі застарілі аналітичні звіти, серпневі прототипи, огляди Manus/Viveka та попередні чернетки переміщено до [`docs/archive/`](archive/README.md). Вони **не є джерелом вимог**.
