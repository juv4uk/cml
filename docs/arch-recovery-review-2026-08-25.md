# Architecture Recovery Review — CML

**Дата:** 2026-08-25 · **Автор:** Vyasa (COMPILER STEWARD)
**Тип:** read-only recovery review · **База:** master @ db51efe..b3ddacc+ (робоча гілка на момент огляду)
**Задача:** ARCH-RECOVERY-REVIEW-CML (my-lisp/tasks.my)
**Ресурси:** читання коду/доків лише; жодних білдів чи тест-прогонів не вимагалось

---

## 1. Що є зараз (as-built pipeline)

```
my-lisp .my ──parse──▶ Expr (my-lisp)
            │
            ▼ lower_program / _with_first_class_builtins   [lower.rs]
            ▼
           Ir  {Int,Buffer,Nil,True,Var,Quote,Lambda,App,PrimOp×7}   [ir.rs]
            │
            ▼ Compiler::compile / compile_with_symbols                    [compiler.rs]
   ├── C backend        (c_backend.rs)
   ├── WGSL emitter     (gpu_wgsl.rs)
   ├── CUDA C emitter   (gpu_cuda.rs)
   └── FPGA asm         (через self-hosted fpga-lisp/assembler.my)

Execution layer:
  compute.rs      — fail-closed аналіз (ElementWise/Scalar shapes, admission facts)
  execution.rs    — heterogeneous graph: data/control edges (0.22+), named errors
  exec_scheduler  — явний NodeExecutor реєстр; unregistered targets fail-closed
  fpga_transport  — CommandFpgaTransport (v1, magic/версія/ліміти, DivisionByZero-сумісні named fails)
  gpu_*_runtime   — CUDA/WGPU adapters (host-staged buffers)
```

**Обсяг:** ~4.4k LOC src, 21 тестових файлів, docs/ = 28 файлів, 153 комміти за 14 днів — проєкт у фазі швидкої інтеграції, і це видно за структурою.

## 2. Сильні сторони (перевірені сьогодні)

1. **Fail-closed дисципліна послідовна**: unregistered targets відхиляються; compute-admission відмовляє при невідомих фактах; planner не підвищує unsupported capabilities.
2. **Named errors замість панік** на execution-межі (Backend/TargetUnavailable/MissingValueProducer/MissingDataDependency/InputKindMismatch).
3. **Evidence-scope машинозчитуваний**: compatibility.my/compute-contract.my несуть machine-readable scope-поля (one-program-path-not-blanket-conformance тощо), contract-lock тести ассертять рядки.
4. **Wire-authority зовнішній**: ISA версії/фрейми делеговані fpga-lisp-isa-1.1, CML не вигадує власну wire-правду.
5. **Self-hosting зсунувся**: harness воліє self-hosted assembler.my (0c51707), compile_with_symbols підключено (dce2103).

## 3. Критичні обмеження Ir (зафіксовані, не баги)

| Обмеження | Джерело | Наслідок |
|---|---|---|
| PrimOp = 7 (Add/Cons/Car/Cdr/Eq/Atom/EqualP) | ir.rs:31-39 | усе інше — через lib/bootstrap або нові варіанти |
| Рядки → символи | compatibility.my notes | строкові значення не виживають lowering на FPGA |
| Нема inexact/rational в ISA | те саме + ecosystem-status 2026-08-11 | BufferLiteral лише I32/F32 |
| Generic calls ≤8 аргументів | notes | перевищення має падати named |
| equal?/defmacro — поза supported-списком cml | compatibility.my | макро-рівень лишається на my-lisp боці |

Це **узгоджено з my-lisp steward-позицією**: семантику диктує language-contract
(зараз 3.0), CML — lowering/backend; розширення Ir = окремі ратифікації.

## 4. Ризики / борги (ранжовано)

1. **MED — execution modularity**: db51efe сам фіксує борг; execution.rs поєднує
   валідацію, планування й dispatch. Розрізати до validate/plan/execute модулів.
2. **MED — transcript/snapshot O(n²)**-подібний патерн у хостах (аналог my-lisp
   e594fd0): перевірити чи CML-хости копіюють повний стан per-request.
3. **LOW — docs drift**: ecosystem-status.my:70 'embedded submodule' застарів
   (cross-repo audit ganaka, 2026-08-24); README physical-scope чесний.
4. **LOW — 8-arg limit** без named-помилки на перевищенні.

## 5. Рекомендації

1. Зробити `validate_graph` публічно-тестовим контрактом (вже майже так).
2. Продовжувати machine-readable evidence-поля (позитивний патерн 2503e95).
3. Ir-розширення (rational buffer?) — тільки після ISA-RATIONAL RTL evidence;
   не випереджати апаратну правду.

---
*Огляд не містить змін коду; усі твердження супроводжені шляхами.*
