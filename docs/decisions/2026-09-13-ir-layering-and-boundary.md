# ADR-004: IR Layering — Semantic IR, Analysis Facts, and Target Representation

**Status:** ACCEPTED  
**Date:** 2026-09-13  
**Authors:** cml team  
**Related Issue:** [#30](https://github.com/juv4uk/cml/issues/30) (`IR-LAYERING-AUDIT-1`)  
**Context:** Auditing shared `Ir` boundaries following Issue #14 Canon Numeric ID dispatch and Issue #27 Capability Matrix reconciliation.

[Українська](#українська) · [English](#english)

---

## Українська

### 1. Контекст та Проблема
Спільний `Ir` у `src/ir.rs` створювався як бекенд-нейтральне семантичне проміжне представлення (middle-end) для `my-lisp`. Однак із розвитком генератора коду для `x86-freestanding` до `Ir` було додано специфічний вузол: `TailSelfCall { args }`.
Він утворюється на етапі `lower.rs` виключно для того, щоб x86 бекенд міг згенерувати цикл зі збереженням фрейму (`jmp .Lloop`) замість `call`.

При цьому:
- Бекенди `c_backend.rs` та `compiler.rs` (FPGA) цей вузол не підтримують і повертають `UnsupportedVariant("TailSelfCall")`.
- `compute.rs` класифікує його як `Stateful`.
- Подальше додавання подібних вузлів для кожної низькорівневої оптимізації (наприклад, unrolling, register hints, spilling, basic blocks) призведе до розмивання семантичної сутності `Ir` та зв'язування незалежних бекендів (FPGA, C, WGPU/CUDA, x86).

### 2. Рішення та Архітектурний Поділ

Встановлюється чітке трирівневе розмежування представлень у компіляторі:

```text
       [ AST / Reader ]
              │
              ▼
    [ 1. Core Semantic IR ] ─── (src/ir.rs)
              │                 • Тільки мовна семантика my-lisp (Canon форми, лямбди, аплікації)
              │                 • Ніяких цільових інструкцій чи бекенд-специфічних вузлів
              ▼
   [ 2. Analysis & Facts ]  ─── (src/compute.rs, майбутній src/analysis.rs)
              │                 • Sidecar-факти, не мутуючі AST/IR:
              │                   - Tail-position analysis
              │                   - Purity / EffectClass
              │                   - Numeric range & overflow proofs
              │                   - StorageClass & parallel candidate shapes
              ▼
     [ 3. Target Lowered ]  ─── (Бекенд-специфічні структури)
                                • x86: CFG / Basic Blocks / Loop Jumps (внутрішній стан src/x86_freestanding/)
                                • FPGA: ISA micro-ops / heap decode layout
                                • GPU: WGSL / PTX kernels (Linearized Kernel IR у compute.rs)
                                • C: C AST / emitter statements
```

### 3. Відповіді на ключові питання аудиту

1. **Чи має `TailSelfCall` залишатися в `Ir`?**
   - **Короткостроково (сьогодні):** Зберігається як перехідний вузол. Працюючий x86 бекенд існуючих тестів не ламаємо завчасно (no big-bang rewrite).
   - **Довгостроково (поріг міграції):** Як тільки з'являється другий споживач tail-call оптимізацій (наприклад, C backend goto-loop або взаємна хвостова рекурсія), `TailSelfCall` вилучається з `Ir`. Замість цього використовується стандартний `Ir::App { func, args }`, а властивість `is_tail_call: bool` передається через аналітичний зріз або x86-локальне зниження в CFG.

2. **Які факти належать `ComputeAnalysis`, а які ядру `Ir`?**
   - До `ComputeAnalysis` належать: `ExecutionShape`, `EffectClass`, `StorageClass`, `NumericDomain`, доведення діапазонів чисел (`OverflowProof`).
   - До `Ir` належать: конструкції значення та зв'язування (`Int`, `Rational`, `String`, `Buffer`, `Lambda`, `App`, `Let`, `Def`, `Prim`).

3. **Коли потрібен цільовий Target Lowered IR для x86/CPU?**
   - Поріг введення: коли x86 backend вимагатиме оптимізації загальних замикань, виділення регістрів для довільних виразів (register allocation / liveness) або переходу до Basic Blocks для довільних `cond` та циклів. Доти поточного AST walk у `src/x86_freestanding.rs` достатньо.

4. **Чи може FPGA продовжувати споживати структурний IR, коли x86 отримає CFG?**
   - Так. FPGA бекенд (`compiler.rs`) природно транслює деревоподібний високорівневий `Ir` у свій асемблер/байткод. Низькорівневий CFG потрібен лише лінійним register-machine бекендам (x86, C).

5. **Як семантичні Canon ID виживають крізь шари IR?**
   - Завдяки `cml#14` (`contracts/cml-operations.my`), Canon ID є числовими непрозорими ідентифікаторами (`CanonOperation.semantic_id`), а не рядками. У всіх шарах (Core IR, Analysis, Target Assembly metadata) операції посилаються на `semantic_id` або варіант `PrimOp`, усуваючи залежність від написання (`car` == `голо́вка`).

---

## English

### 1. Context & Problem
The shared `Ir` in `src/ir.rs` serves as the backend-neutral semantic middle-end for `my-lisp`. However, to support bounded self-recursion on x86, a target-specific variant was introduced: `TailSelfCall { args }`.
This variant is manufactured during `lower.rs` purely so the x86 backend can emit a register-reloading loop jump (`jmp .Lloop`) instead of a stack-growing `call`.

Trade-offs:
- `c_backend.rs` and `compiler.rs` (FPGA) reject `TailSelfCall` with `UnsupportedVariant`.
- `compute.rs` classifies it as `Stateful`.
- Adding ad-hoc variants for every downstream backend optimization leaks low-level execution details into the shared language semantics.

### 2. Decision & Architectural Layering

A three-tier compilation model is established:

```text
    1. Core Semantic IR (src/ir.rs)
       - Strict representation of my-lisp language semantics.
       - No target-dependent instructions or backend-specific variants.

    2. Analysis & Annotation Layer (src/compute.rs, future analysis passes)
       - Non-destructive sidecar facts:
         * Tail position identification
         * Purity and side-effects (EffectClass)
         * Storage layout and contiguous buffer requirements
         * Integer range proofs for overflow-free execution

    3. Target Lowered Representation (backend-internal)
       - x86: Basic blocks, jump-targets, loop labels, frame reload sequences
       - FPGA: Microcode / heap word emitters
       - GPU: WGSL / PTX kernels
       - C: C source statements and expressions
```

### 3. Answers to Audit Questions

1. **Should `TailSelfCall` remain an IR node?**
   - **Current status:** Remains in `Ir` temporarily to preserve existing working, verified code without premature churn.
   - **Migration threshold:** When general tail-call elimination or a second backend (C `goto` loop) is added, `TailSelfCall` will be removed from `Ir`. An analysis pass or lowered target pass will annotate or convert `Ir::App` in tail position.

2. **Which facts belong in `ComputeAnalysis` vs Core `Ir`?**
   - `ComputeAnalysis`: Proven facts such as execution shape, purity, memory locality, and numeric overflow safety.
   - Core `Ir`: AST reduction, bindings, definitions, and primitive forms.

3. **When is a lowered Target IR needed for x86?**
   - Explicit threshold: when introducing register allocation across non-trivial control-flow graphs or multi-block scheduling.

4. **Can FPGA keep consuming structural IR while x86 gets a lower stage?**
   - Yes. The shared middle-end can fork after the analysis stage: FPGA directly lowers tree-shaped `Ir`, while x86/CPU lowers to a CFG or basic block sequence.

5. **How do Canon semantic IDs survive across IR layers?**
   - Canon IDs are opaque 32-bit integers derived from `semantic-registry.wsm` (per Issue #14). They are carried by metadata and dispatch tables without ever reverting to string-based surface matching.
