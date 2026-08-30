# cml as a backend-independent compiler core

Status: strategy agreed 2026-08-12 (owner + opencode engineer); steps 1
and 2 of the incremental path below are now implemented (`ir.rs`/
`lower.rs`, `compiler.rs` consuming `Ir`, `src/c_backend.rs`) — see
"Current reality" for what actually exists today, not just the plan.
This file still exists so a "CUDA backend" (step 3, not started) can't
quietly become a second independent compiler once someone picks it up.

## The goal, stated once

> **my-lisp has one semantics and several physical forms of execution:
> CPU (C), GPU (CUDA), and FPGA (Verilog).**

`cml` is the middle-end for all three. The alternative — a my-lisp→C
compiler, a my-lisp→CUDA compiler, and a my-lisp→FPGA compiler as three
separate projects — is the anti-pattern this document exists to prevent.

## The whole picture

```
                my-lisp
                   │
             semantic IR
                   │
       ┌───────────┼───────────┐
       ▼           ▼           ▼
       C          CUDA       Verilog
       │           │           │
      CPU         GPU         FPGA
```

Compilation pipeline:

```
source → reader → macro expansion → semantic analysis → common IR
                                                          │
                                            ┌─────────────┼─────────────┐
                                            ▼             ▼             ▼
                                          C backend    CUDA backend   fpga-lisp backend
```

The one thing that makes all three backends shareable is a *common,
backend-neutral IR* in the middle. Everything below follows from having
that IR; nothing above it changes.

### Current semantic gate

`src/semantic.rs` is the first fail-closed admission pass between macro
expansion and lowering. It deliberately does **not** claim full my-lisp 3.0
semantic analysis. Its current executable boundary rejects two shapes that
CML previously could compile into a different program than the canonical
evaluator:

- duplicate lambda parameters, including collisions introduced by CML's
  current uppercase symbol representation;
- lambda bodies containing more than one expression, because canonical
  my-lisp evaluates them sequentially while the current `Ir::Lambda` can
  represent exactly one body expression.

Both `lower_program` and the public `lower_expr` entry point apply this gate,
so a source consumer cannot silently bypass it by selecting a backend. Further
contract coverage remains open work; rejection here is evidence of safe
non-support, not a claim that the full semantic-analysis milestone is done.

## Why purity is the enabler

my-lisp's functional, immutable semantics are the property that makes
this tractable:

- a pure function is trivially data-parallel → `(map square xs)` can
  become a CUDA kernel without alias analysis;
- a pure function is trivially a dataflow → `x → op A → op B → result`
  with no global-memory model, which is exactly what an FPGA wants;
- sequential / branching-heavy code stays on the CPU.

So the language's "encourages pure code" property is not a style bonus —
it is the architectural precondition for multi-target execution.

## Current Executable State (August 2026)

`cml` is a **multi-backend** heterogeneous compiler sharing one semantic IR:

- `src/parser.rs` → `src/ast.rs` → macro expansion → `src/lower.rs` → `ir::Ir` (`src/ir.rs`)
- Backends:
  - **FPGA (`src/compiler.rs`)**: fpga-lisp ISA, hardware-verified.
  - **C (`src/c_backend.rs`)**: Hosted C source, verified against the my-lisp oracle.
  - **x86_64 Freestanding (`src/x86_freestanding.rs`)**: Target ABI compatible GNU assembly for `wsm-os`, featuring bounded fail-closed semantics.
  - **CUDA (`src/gpu_cuda.rs` / `src/gpu_cuda_runtime.rs`)**: Emits and launches PTX compute kernels based on explicit execution graph analysis.
  - **WGPU (`src/gpu_wgsl.rs` / `src/gpu_wgpu_runtime.rs`)**: Portable WGSL compute backend (optional).
- `Ir` covers: literals, `nil`/`t`, variables, `quote`, `lambda`, application, `cond`, `let`, `def`, primitives, and new Compute representations (`Map`, `Reduce`, `Scan`, `Index`, `ParallelRegion`). It also features `Buffer` (I32/F32) and `TailSelfCall`.
- Neither backend supports rationals/bignums/inexact numbers yet (`compatibility.my`'s `limitations`).
- Backends explicitly fail-closed in a preflight validation step (`validate_ir`), meaning if an IR variant (like `Ir::Builtin`) is not supported, it is rejected with a typed `CompileError` rather than silently admitted and panicked upon.

The Execution Graph (in `src/execution*.rs`) now orchestrates these nodes, ensuring typed buffers correctly map between CPU host logic and GPU compute kernels, without claiming direct GPU-to-FPGA transfers.

## Historical Plan & Architecture Path (Preserved)

### Symbol ABI bridge (2026-08-24)

`Compiler::compile_with_symbols` assigns per-program `LOADSYM` IDs from 900.
The self-hosted `fpga-lisp/assembler.my` now implements the same normalization
and `.sym` sidecar contract. `tests/compiler_test.rs` includes a cross-repo
fixture that assembles CML output through both `assembler.py` and
`assembler.my` and requires identical bytes. This is an implementation-parity
proof only; it does not transfer language or backend authority to either
assembler. The compiler-test harness now prefers the release `my-lisp`
assembler and falls back to Python only when that binary is unavailable.

1. **Draw the backend boundary inside cml.** ✅ Done: a backend-neutral `Ir` covering every form `compile_*` in `compiler.rs` handles.
2. **C backend next, not CUDA.** ✅ Done: `src/c_backend.rs`, a small tagged-union `Value` runtime.
3. **Compute analysis after C.** ✅ Done: M0 implemented in `src/compute.rs`, GPU admission fails closed.
4. **Portable GPU backend after a typed-buffer contract.** ✅ Done: WGPU and CUDA backends implemented and integrated into the execution graph.
5. **fpga-lisp stays as a backend** of the same semantic IR, with a future dataflow lowering as a separate specialization path.

Later, the target can even be *chosen by the compiler* when provably safe, and a single program can span all three:

```lisp
(let ((raw (fpga-read)))
  (let ((processed (gpu-map transform raw)))
    (cpu-decide processed)))
```

## Swarm mapping

The four-repo swarm already maps onto this: `my-lisp` (language/semantic
source of truth), `cml` (compiler middle-end), `fpga-lisp` (FPGA
backend), `my-idea` (observatory). New execution backends (C, CUDA, x86_64) may
each become their own node in the P2P mesh — the mesh is designed for
new members to join with a single `--connect` (see my-lisp
`docs/swarm-mesh-v2.md`).

All backends are judged against the same semantic contract
(`language-contract.my` / `isa-contract.my` / `compatibility.my`), not
against each other's implementation.

## Non-goals for now

- No GPU in the build toolchain (Guix does not package the CUDA toolkit; nvcc stays a host-side Ubuntu tool).
- No change to fpga-lisp's contract or to `:9999` semantics.

---

# cml як незалежне від бекендів ядро компілятора (Ukrainian)

Статус: стратегія узгоджена 2026-08-12 (власник + інженер opencode); кроки 1
та 2 інкрементального плану нижче вже реалізовані (`ir.rs`/`lower.rs`, 
`compiler.rs` споживає `Ir`, `src/c_backend.rs`) — див. "Поточний стан", щоб
дізнатися, що саме існує на сьогодні, а не лише план. Цей файл зберігається
для того, щоб "CUDA backend" (крок 3, ще не розпочато) не зміг непомітно
перетворитися на другий незалежний компілятор.

## Мета, сформульована один раз

> **my-lisp має одну семантику та кілька фізичних форм виконання:
> CPU (C), GPU (CUDA), і FPGA (Verilog).**

`cml` — це middle-end для всіх трьох. Альтернатива — компілятор my-lisp→C,
компілятор my-lisp→CUDA і компілятор my-lisp→FPGA як три окремі проєкти —
є антипатерном, якому цей документ має запобігти.

## Загальна картина

```
                my-lisp
                   │
             семантичний IR
                   │
       ┌───────────┼───────────┐
       ▼           ▼           ▼
       C          CUDA       Verilog
       │           │           │
      CPU         GPU         FPGA
```

Пайплайн компіляції:

```
сирці → reader → макророзширення → семантичний аналіз → спільний IR
                                                          │
                                            ┌─────────────┼─────────────┐
                                            ▼             ▼             ▼
                                          C backend    CUDA backend   fpga-lisp backend
```

Єдине, що дозволяє всім трьом бекендам бути спільними — це *спільний, 
незалежний від бекенда IR* посередині. Все, що описано нижче, випливає 
з наявності цього IR; все, що вище, не змінюється.

### Поточний семантичний бар'єр (Semantic gate)

`src/semantic.rs` — це перший fail-closed прохід допуску між розширенням
макросів та зниженням (lowering). Він навмисно **не** претендує на повний
семантичний аналіз `my-lisp 3.0`. Його поточна межа відхиляє дві форми,
які раніше `cml` міг скомпілювати в програму, що відрізняється від 
результату канонічного інтерпретатора:

- параметри лямбди, що дублюються, зокрема колізії, внесені поточним 
  представленням символів у CML у верхньому регістрі;
- тіла лямбди, що містять більше одного виразу, оскільки канонічний `my-lisp`
  обчислює їх послідовно, тоді як поточний `Ir::Lambda` може представляти 
  лише один вираз-тіло.

І `lower_program`, і публічна точка входу `lower_expr` застосовують цей бар'єр,
тому споживач (source consumer) не може непомітно обійти його шляхом 
вибору бекенда. Подальше покриття контракту залишається відкритою задачею; 
відхилення тут є доказом безпечної непідтримки, а не заявою, що етап повного
семантичного аналізу завершено.

## Чому чистота є фактором забезпечення (enabler)

Функціональна, незмінна семантика my-lisp — це та властивість, яка робить 
це можливим:

- чиста функція є тривіально паралельною для даних → `(map square xs)` може
  стати ядром CUDA без аналізу псевдонімів (alias analysis);
- чиста функція є тривіальним потоком даних → `x → op A → op B → result` 
  без глобальної моделі пам'яті, що є саме тим, чого вимагає FPGA;
- послідовний / сильно розгалужений код залишається на CPU.

Отже, властивість мови "заохочує чистий код" — це не просто стилістичний
бонус, це архітектурна передумова для багатоцільового виконання 
(multi-target execution).

## Поточний стан виконання (Серпень 2026)

`cml` — це **multi-backend** гетерогенний компілятор, що використовує один семантичний IR:

- `src/parser.rs` → `src/ast.rs` → макророзширення → `src/lower.rs` → `ir::Ir` (`src/ir.rs`)
- Бекенди:
  - **FPGA (`src/compiler.rs`)**: fpga-lisp ISA, апаратно перевірено.
  - **C (`src/c_backend.rs`)**: Hosted C сирці, перевірено проти my-lisp оракула.
  - **x86_64 Freestanding (`src/x86_freestanding.rs`)**: GNU асемблер для `wsm-os`, з fail-closed семантикою.
  - **CUDA (`src/gpu_cuda.rs` / `src/gpu_cuda_runtime.rs`)**: Генерує та запускає PTX обчислювальні ядра (compute kernels) на базі явного аналізу графа виконання.
  - **WGPU (`src/gpu_wgsl.rs` / `src/gpu_wgpu_runtime.rs`)**: Портативний WGSL compute backend (опціонально).
- `Ir` покриває: літерали, `nil`/`t`, змінні, `quote`, `lambda`, аплікацію, `cond`, `let`, `def`, примітиви, а також нові Compute-представлення (`Map`, `Reduce`, `Scan`, `Index`, `ParallelRegion`). Також має `Buffer` (I32/F32) і `TailSelfCall`.
- Жоден з бекендів поки не підтримує rationals/bignums/неточні числа (`limitations` у `compatibility.my`).
- Бекенди явно fail-closed на етапі попередньої перевірки (`validate_ir`), що означає: якщо варіант IR не підтримується, він відхиляється через типізований `CompileError`, а не мовчки допускається і викликає паніку.

Граф виконання (в `src/execution*.rs`) тепер керує цими вузлами, гарантуючи, що
типізовані буфери коректно передаються між хост-логікою CPU та GPU-ядрами обчислень,
без заяв про пряму передачу з GPU на FPGA.

## Історичний план та архітектурний шлях (Збережено)

### Міст ABI символів (2026-08-24)

`Compiler::compile_with_symbols` присвоює ID `LOADSYM` для програми, починаючи
з 900. Самохостний (self-hosted) `fpga-lisp/assembler.my` тепер реалізує таку саму
нормалізацію та `.sym` контракт. Фікстура тестування доводить ідентичність
байтів обох асемблерів. Тестова оболонка тепер надає перевагу `my-lisp` асемблеру.

1. **Провести межу бекенда всередині cml.** ✅ Виконано: backend-neutral `Ir`.
2. **C backend наступний, не CUDA.** ✅ Виконано: `src/c_backend.rs`, невеликий runtime з tagged-union `Value`.
3. **Compute аналіз після C.** ✅ Виконано: M0 реалізовано в `src/compute.rs`, GPU допуск fails closed.
4. **Портативний GPU backend після контракту типізованого буфера.** ✅ Виконано: інтегровано WGPU та CUDA.
5. **fpga-lisp залишається бекендом** для того ж семантичного IR, з майбутнім зниженням dataflow як окремим шляхом спеціалізації.

Пізніше ціль може навіть *обиратися компілятором*, коли це доведено безпечно:

```lisp
(let ((raw (fpga-read)))
  (let ((processed (gpu-map transform raw)))
    (cpu-decide processed)))
```

## Відповідність рою (Swarm mapping)

Мережа з 4 репозиторіїв вже відповідає цьому: `my-lisp`, `cml`, `fpga-lisp`, `my-idea`. 
Нові бекенди (C, CUDA, x86_64) можуть стати власними вузлами в P2P-мережі. Усі
бекенди оцінюються щодо одного семантичного контракту, а не один проти одного.

## Цілі, що поки відкладені (Non-goals)

- Ні GPU в інструментарії збірки (nvcc залишається інструментом Ubuntu на боці хоста).
- Ніяких змін у контракті `fpga-lisp` або в семантиці `:9999`.
