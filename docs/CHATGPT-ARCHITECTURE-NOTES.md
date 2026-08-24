# Imported ChatGPT architecture notes

This is a progressive archive of material supplied by Volodymyr from a separate ChatGPT conversation. Imported passages are preserved as proposals, not silently promoted into repository contracts.

## Entry 001 — CUDA as a heterogeneous execution backend

- Imported: 2026-08-24
- Supplied by: Volodymyr
- Original speaker: ChatGPT (separate conversation)
- Status: **PROPOSAL / UNVERIFIED**
- Relevant repositories: `my-lisp`, `cml`
- Authority boundary: `my-lisp` remains the canonical language semantics; `cml` is the candidate compiler/IR layer. Nothing in this entry changes either contract.

### Viveka index

The proposal argues for a typed, contiguous numeric array domain; an explicit exact/inexact boundary; a small pure GPU-safe subset; an explicit CUDA backend before automatic offload; and differential testing across independently lowered CPU/CUDA/FPGA backends. Its strongest architectural question is whether CUDA lowering belongs in `cml`, while host capability discovery remains outside canonical semantics.

### Imported passage

> Друже, глянув актуальну структуру `my-lisp`, і тут я бачу дуже природний шлях до CUDA — **не “переписати Lisp під GPU”**, а додати окремий execution backend для тих виразів, які справді мають масовий паралелізм.
>
> Зараз ядро у тебе вже добре розкладене по crates: `my-lisp` як canonical runtime, окремо `my-lisp-host`, `my-lisp-wasm`, LSP, semantic, swarm-node. Це хороший фундамент для ще одного backend-layer, а не для вбудовування CUDA прямо в evaluator.
>
> І ще важливіше: числова модель мови зараз **exact by default** — власний `BigInt`, exact `Rational`, без float-rounding як базової семантики. Це прямо видно з `value.rs`. Тому GPU-шлях треба робити так, щоб він **не підміняв точну семантику мовчки**.
>
> ```text
> my-lisp source
>     ↓
> evaluator / compiler
>     ↓
> detect parallel numeric region
>     ↓
> ExecutionPlan
>    ├─ CPU exact
>    └─ GPU CUDA
>           ↓
>       kernel launch
>           ↓
>       result back to Value
> ```
>
> Ключове: CUDA — **не нова семантика**, а альтернативний спосіб виконання строго визначеного підмножини задач.
>
> ### Що реально добре лягає на GPU
>
> Не `cons`, не `car`, не макроси, не рекурсивний evaluator. А такі речі:
>
> ```lisp
> (map f big-vector)
> (reduce + big-vector)
> (zip-with * a b)
> (matrix-mul a b)
> (dot a b)
> (convolve signal kernel)
> ```
>
> Тобто масивні числові операції.
>
> А от твої linked lists і exact arbitrary-precision rationals — для CUDA незручні. GPU любить:
>
> ```text
> contiguous memory
> fixed-width numbers
> same operation over many elements
> ```
>
> Тому перший важливий крок — **не CUDA**, а числовий масив як окремий тип.
>
> Наприклад:
>
> ```rust
> Value::Tensor(TensorValue)
> ```
>
> або простіше:
>
> ```rust
> Value::Array(ArrayValue)
> ```
>
> де:
>
> ```rust
> enum ElementType {
>     I32,
>     I64,
>     F32,
>     F64,
> }
> ```
>
> І тут треба бути дуже чесним із твоєю мовою: `F32/F64` не можна видавати за exact `Rational`.
>
> Я б зробив це видимим:
>
> ```lisp
> (array 'f32 1 2 3 4)
> ```
>
> або
>
> ```lisp
> (inexact-array 1 2 3 4)
> ```
>
> Тоді користувач сам переходить у inexact domain.
>
> А exact:
>
> ```lisp
> (list 1/3 2/7 ...)
> ```
>
> залишається CPU path.
>
> Це дуже важлива межа:
>
> ```text
> exact domain
>     ↓
> CPU canonical semantics
>
> inexact bulk numeric domain
>     ↓
> CPU or GPU backend
> ```
>
> Тоді CUDA не порушує твою основну філософію.
>
> ---
>
> Я б не писав CUDA прямо в `crates/my-lisp`.
>
> Краще:
>
> ```text
> crates/
>   my-lisp/
>   my-lisp-host/
>   my-lisp-wasm/
>   my-lisp-cuda/
> ```
>
> `my-lisp-cuda` може залежати від CUDA runtime, але canonical core — ні.
>
> Це ще й важливо для WASM: браузерна збірка не повинна навіть знати, що CUDA існує.
>
> Тоді capability-модель:
>
> ```lisp
> (capability? 'cuda)
> ```
>
> і:
>
> ```text
> Linux/NVIDIA:
>   cuda = available
>
> Windows/NVIDIA:
>   cuda = available
>
> WASM:
>   cuda = unavailable
>
> machine without NVIDIA:
>   cuda = unavailable
> ```
>
> Тобто GPU — capability host/backend, а не language primitive.
>
> ---
>
> Ще цікавіший варіант — не робити користувача залежним від CUDA у source code.
>
> Наприклад:
>
> ```lisp
> (parallel-map square xs)
> ```
>
> або:
>
> ```lisp
> (map square xs)
> ```
>
> а planner сам вирішує:
>
> ```text
> N < threshold
> → CPU
>
> N >= threshold
> + f is GPU-safe
> + CUDA available
> → GPU
> ```
>
> Оце мені подобається найбільше.
>
> Тоді програма залишається Lisp-програмою:
>
> ```lisp
> (map square xs)
> ```
>
> а execution backend — implementation detail.
>
> Але автоматичний offload я б не робив у M0. Спочатку explicit:
>
> ```lisp
> (gpu-map square xs)
> ```
>
> або краще capability API:
>
> ```lisp
> (with-backend 'cuda
>   (map square xs))
> ```
>
> Поки не буде доказано, що planner стабільно робить правильний вибір.
>
> ---
>
> Тут ще є принципове питання: **що таке GPU-safe function**?
>
> Наприклад:
>
> ```lisp
> (lambda (x)
>   (* x x))
> ```
>
> легко.
>
> А:
>
> ```lisp
> (lambda (x)
>   (cons x global-state))
> ```
>
> ні.
>
> Тому потрібен дуже малий pure subset:
>
> ```text
> GPU kernel may use:
> - fixed-width arithmetic
> - comparisons
> - cond
> - local bindings
> - pure functions
> - no allocation
> - no mutation
> - no IO
> - no host capability
> - no arbitrary recursion
> ```
>
> Це можна перевіряти перед compilation.
>
> Фактично:
>
> ```text
> Lisp lambda
>    ↓
> purity / kernel eligibility checker
>    ↓
> GPU IR
>    ↓
> CUDA
> ```
>
> І тут дуже природно підключається `cml`.
>
> Бо `cml` у твоїй екосистемі вже є compiler/AOT/IR-напрямком. Я б навіть подумав, чи CUDA backend **не повинен жити саме там**, а не в runtime.
>
> Тобто:
>
> ```text
> my-lisp
>   = semantics
>
> cml
>   = compiler / IR
>
> cml-cuda backend
>   = CUDA lowering
> ```
>
> Це концептуально чистіше.
>
> Наприклад:
>
> ```lisp
> (map
>   (lambda (x) (+ (* x x) 1))
>   xs)
> ```
>
> компілюється в IR:
>
> ```text
> load x
> mul x x
> add 1
> store
> ```
>
> а backend:
>
> ```text
> CPU
> CUDA
> FPGA
> WASM
> ```
>
> Тоді в тебе починає вимальовуватися дуже цікава штука:
>
> ```text
>                  my-lisp semantics
>                        ↓
>                       cml IR
>         ┌──────────────┼──────────────┐
>         ↓              ↓              ↓
>        CPU            CUDA           FPGA
> ```
>
> Оце вже серйозна архітектура.
>
> І важливо не зробити C11-помилку: CPU/CUDA/FPGA тут **не same engine**. Вони будуть independent/lowered execution backends, які треба перевіряти conformance-тестами проти canonical semantics.
>
> ---
>
> Для CUDA M0 я б не брав matrix multiplication. Візьми найменший proof:
>
> ```lisp
> (gpu-map
>   (lambda (x) (* x x))
>   vector)
> ```
>
> і pipeline:
>
> ```text
> Lisp lambda
> → validate pure numeric subset
> → translate to CUDA kernel
> → copy vector host→device
> → execute
> → copy device→host
> → convert back to Lisp array
> ```
>
> Acceptance tests:
>
> ```text
> CPU result == GPU result
> ```
>
> для `i32/i64` — byte/element exact.
>
> Для `f32/f64`:
>
> ```text
> within defined tolerance
> ```
>
> І дуже важливо — не називати floating result exact.
>
> ---
>
> Я б дав агенту такий план:
>
> ```text
> CUDA M0 — proof of offload
>
> 1. No changes to canonical semantics.
> 2. Introduce contiguous numeric Array/Tensor value.
> 3. Explicit inexact/fixed-width element type.
> 4. Define GPU-safe pure subset.
> 5. Add backend trait:
>    trait ComputeBackend {
>        fn map(...)
>        fn reduce(...)
>    }
>
> 6. Implement CPU backend first.
> 7. Use CPU backend as reference for this subset.
> 8. Implement CUDA backend separately.
> 9. Differential tests CPU vs CUDA.
> 10. Only after correctness: automatic backend selection.
> ```
>
> І ще один нюанс через твою GTX 1050 Ti: вона чудово підійде для M0/M1 експериментів, але PCIe transfer overhead означає, що на малих масивах GPU буде **повільніший** за CPU. Тому потім planner має враховувати:
>
> ```text
> transfer cost
> kernel launch cost
> N
> arithmetic intensity
> ```
>
> Грубо:
>
> ```text
> if estimated_gpu_time < estimated_cpu_time
>     GPU
> else
>     CPU
> ```
>
> Але це вже M2.
>
> Моя головна думка: **не робити “CUDA-функції” частиною ядра мови**. Зроби поняття:
>
> ```text
> pure parallel computation
> ```
>
> а CUDA нехай буде лише одним із backend'ів.
>
> Тоді завтра замість CUDA можна додати:
>
> ```text
> ROCm
> Vulkan compute
> WebGPU
> FPGA
> ```
>
> не змінюючи Lisp-програми.
>
> І це дуже в дусі твоєї архітектури:
>
> ```text
> language says WHAT
> backend decides WHERE
> ```
>
> Оце я б вважав правильним напрямком для `my-lisp`.

### Verification queue

- Re-check the claimed current crate layout and exact numeric representation against the live `my-lisp` tree.
- Decide whether a fixed-width array is a language-level value, an FFI/host value, or a compiler-only region type; these choices are not equivalent.
- Compare the proposed pure subset with the existing backend-neutral `cml` IR before designing a new GPU IR.
- Specify overflow semantics for `i32/i64` and a reproducible comparison policy for `f32/f64`.
- Keep explicit offload as the first experiment; automatic selection requires both semantic eligibility and a cost model.
- Treat CPU, CUDA, FPGA, and WASM results as independently produced evidence, not as one engine validating itself.

## Entry 002 — Backend-neutral universal computation model

- Imported: 2026-08-24
- Supplied by: Volodymyr
- Original speaker: ChatGPT (separate conversation)
- Status: **PROPOSAL / UNVERIFIED**
- Relevant repositories: `my-lisp`, `cml`
- Relation to Entry 001: sharpens the abstraction from “CUDA backend” to a backend-neutral execution model.
- Authority boundary: neutral source syntax does not by itself prove that backend selection preserves the canonical language contract.

### Viveka index

The passage separates three responsibilities: the language describes what to compute; an IR/planner establishes how it may be parallelized; a backend selects where it runs. It proposes that semantic eligibility be expressed in neutral properties such as `pure`, `parallel`, `numeric`, and `bulk`, with CUDA treated only as one NVIDIA implementation beneath the GPU class.

### Imported passage

> Так, саме так.
>
> Для **універсальної мови** тобі не потрібна CUDA як частина самої мови. CUDA — це лише **один конкретний backend для NVIDIA**.
>
> Правильніше мислити так:
>
> ```text
> my-lisp
>   ↓
> вираз / обчислення
>   ↓
> execution planner
>   ↓
> де виконати?
>   ├─ CPU
>   ├─ GPU
>   ├─ FPGA
>   ├─ WASM
>   └─ інший backend
> ```
>
> Тобто мова повинна вміти описати:
>
> > «Ось чисте паралельне обчислення над великим масивом даних».
>
> А вже runtime/compiler вирішує:
>
> > «На цій машині вигідніше виконати це на відеокарті».
>
> Наприклад, source може залишатися таким:
>
> ```lisp
> (map
>   (lambda (x) (* x x))
>   data)
> ```
>
> А planner бачить:
>
> ```text
> data = великий numeric array
> lambda = pure
> GPU = available
> backend supports this operation
> ```
>
> і відправляє це на GPU.
>
> На іншому комп'ютері без NVIDIA:
>
> ```text
> same Lisp program
> → CPU
> ```
>
> На AMD:
>
> ```text
> same Lisp program
> → ROCm/Vulkan/WebGPU backend
> ```
>
> На твоїй NVIDIA:
>
> ```text
> same Lisp program
> → CUDA backend
> ```
>
> Оце і є універсальність.
>
> Я б навіть розділив три поняття:
>
> ```text
> Language
>     WHAT should be computed
>
> IR / planner
>     HOW computation can be parallelized
>
> Backend
>     WHERE it is actually executed
> ```
>
> Тоді CUDA взагалі не повинна просочуватися в семантику `my-lisp`.
>
> Можна навіть мати нейтральне поняття:
>
> ```lisp
> (parallel-map f xs)
> ```
>
> або згодом взагалі просто звичайний `map`, якщо compiler доведе, що його безпечно паралелити.
>
> Ключове слово тут не **CUDA**, а **compute backend**.
>
> І це відкриває значно цікавішу архітектуру:
>
> ```text
>                    my-lisp
>                       │
>                   semantic IR
>                       │
>           ┌───────────┼───────────┐
>           │           │           │
>          CPU         GPU         FPGA
>                       │
>             ┌─────────┼─────────┐
>             │         │         │
>            CUDA     ROCm      WebGPU
> ```
>
> Тобто GPU — це клас пристрою, а CUDA — лише один спосіб ним скористатися.
>
> І навіть ще точніше: мові не обов'язково знати навіть слово `GPU`. Вона може знати лише:
>
> ```text
> pure
> parallel
> numeric
> bulk
> ```
>
> А execution system уже вирішить, де це рахувати.
>
> Оце, на мою думку, значно ближче до твоєї цілі універсальної Lisp-машини:
>
> > **програма описує обчислення, а машина сама знаходить відповідне залізо.**
>
> Саме так я б і будував напрямок далі.

### Verification queue

- Define whether `parallel-map` has distinct language semantics or is only an optimization promise/hint.
- Prove that ordinary `map` may be reordered or parallelized only where effects, errors, allocation, and evaluation order remain observationally equivalent.
- Keep semantic eligibility separate from profitability: a valid GPU region may still be faster on CPU.
- Establish which layer owns device discovery, capability negotiation, scheduling, fallback, and failure reporting.
- Test each lowering independently against canonical semantics; a shared semantic IR is a specification boundary, not proof of backend equivalence.

## Entry 003 — CML execution domains and compute classes

- Imported: 2026-08-24
- Supplied by: Volodymyr
- Original speaker: ChatGPT (separate conversation)
- Status: **PROPOSAL / UNVERIFIED**
- Relevant repositories: primarily `cml`; semantic boundary owned by `my-lisp`
- Relation to Entries 001–002: proposes a backend-independent classification layer inside CML before device selection.

### Viveka index

The new concept is an `execution domain` or `compute class`: `scalar`, `bulk-parallel`, `reduction`, `stream`, `pipeline`, or `stateful`. CML would recognize the computational shape, lower it to a neutral parallel region, and only then let CPU, GPU, or FPGA backends choose mechanisms such as SIMD/threads, kernels, or pipelines.

### Imported passage

> Це дуже логічно. Саме **CML** виглядає природним місцем, де така думка мала виникнути, бо там ти вже мислиш не “як інтерпретувати Lisp”, а “як один і той самий зміст опустити на різні обчислювальні машини”.
>
> Я б це сформулював так:
>
> ```text
> my-lisp
> = семантика мови
>
> cml
> = lowering / IR / compiler decisions
>
> backend
> = конкретне залізо
> ```
>
> Тоді GPU стає просто ще одним напрямком поряд із FPGA:
>
> ```text
>                 my-lisp
>                    ↓
>                  CML IR
>           ┌────────┼─────────┐
>           ↓        ↓         ↓
>          CPU      GPU       FPGA
>                    │
>             CUDA / ROCm / ...
> ```
>
> І це красиво ще й тому, що GPU та FPGA дуже різні фізично, але на рівні CML для них можна шукати спільне поняття:
>
> > **“цей фрагмент обчислення має достатньо явний паралелізм і мало побічних ефектів, щоб його можна було винести з послідовного evaluator.”**
>
> Наприклад:
>
> ```lisp
> (map
>   (lambda (x) (+ (* x x) 1))
>   data)
> ```
>
> CML може побачити не Lisp-список як такий, а щось на кшталт:
>
> ```text
> parallel-region
>   input: numeric-buffer
>   operation:
>     mul
>     add
>   dependency: element-local
> ```
>
> І далі:
>
> ```text
> CPU backend  → SIMD / threads
> GPU backend  → compute kernel
> FPGA backend → pipeline
> ```
>
> Оце, на мою думку, значно сильніше за “додамо CUDA в Lisp”.
>
> Фактично CML може стати тим шаром, який відповідає на питання:
>
> > **Яка форма обчислення перед нами?**
>
> а вже не:
>
> > **На якому бренді відеокарти ми її запустимо?**
>
> І тут є дуже цікава симетрія з твоєю Lisp-машиною на FPGA: GPU не конкурент FPGA. Це інший тип спеціалізованого виконавця.
>
> Грубо:
>
> ```text
> CPU   — складний контроль, загальна логіка
> GPU   — масова однотипна паралельність
> FPGA  — спеціалізований потік / власна архітектура
> ```
>
> А CML може бути диспетчером між цими світами.
>
> Тому я б навіть дав цьому окрему концепцію в CML, не прив’язану до GPU:
>
> ```text
> execution domain
> ```
>
> або
>
> ```text
> compute class
> ```
>
> Наприклад:
>
> ```text
> scalar
> bulk-parallel
> reduction
> stream
> pipeline
> stateful
> ```
>
> Тоді backend selection стає майже механічним:
>
> ```text
> bulk-parallel
> → GPU candidate
>
> stream/pipeline
> → FPGA candidate
>
> irregular/stateful
> → CPU candidate
> ```
>
> І ось це вже справді схоже на універсальну мову: не тому, що вона “підтримує все залізо”, а тому, що **вона описує обчислення на достатньо високому рівні, щоб компілятор міг вибирати відповідну машину**.

### Verification queue

- Compare the proposed compute classes with the live CML IR instead of adding a parallel type system prematurely.
- Specify whether a compute class is inferred evidence, an explicit source annotation, or both, and how conflicts are reported.
- Model data layout, transfer boundaries, dependencies, ordering, effects, and failure semantics; operation shape alone is insufficient for safe placement.
- Treat `bulk-parallel → GPU`, `stream/pipeline → FPGA`, and `irregular/stateful → CPU` as planner candidates, not semantic laws.
- Determine whether `reduction` requires associativity, commutativity, a fixed tree/order, or a backend-specific numeric contract.

## Entry 004 — Claimed live CML architecture and Heterogeneous M0

- Imported: 2026-08-24
- Supplied by: Volodymyr
- Original speaker: ChatGPT (separate conversation)
- Status: **PROPOSAL with CLAIMED CURRENT STATE / NOT YET VERIFIED BY VIVEKA**
- Relevant repositories: primarily `cml`; conformance authority anchored in `my-lisp`
- Relation to Entries 001–003: claims the backend-neutral direction already exists in live CML and proposes an analysis-only M0 before CUDA implementation.

### Viveka index

The passage claims that CML already has a backend-neutral IR, C and FPGA emitters, heterogeneous-backend documentation, and separated lowering/backend/conformance tests. It identifies a gap between the current Lisp-oriented semantic IR and the buffer/index/parallel-region representation useful to accelerators. The proposed response is a second execution-domain or Compute IR, preceded by effect and execution-shape analysis. Because these are claims about current files and test history, they remain unverified until checked directly against the repository and evidence.

### Imported passage

> Друже, глянув актуальний `cml`. І тепер видно, що твоя думка про GPU там **вже не просто природна — вона фактично закладена в архітектуру**.
>
> `cml` зараз не просто «компілятор my-lisp у FPGA assembly». У репо вже є backend-neutral IR, окремий C backend і документ `heterogeneous-backends.md`, де прямо записана ціль: **одна семантика my-lisp, кілька фізичних форм виконання — CPU, GPU, FPGA**.
>
> Найважливіша схема вже буквально є в документації:
>
> ```text
>                 my-lisp
>                    │
>              semantic IR
>                    │
>        ┌───────────┼───────────┐
>        ▼           ▼           ▼
>        C          CUDA       Verilog
>        │           │           │
>       CPU         GPU         FPGA
> ```
>
> І документ спеціально застерігає: **не робити окремий `my-lisp → CUDA compiler`**, щоб CUDA не стала другим незалежним компілятором і не розійшлася із семантикою. `cml` має бути єдиним middle-end.
>
> ### Що вже реально зроблено
>
> Поточний pipeline вже такий:
>
> ```text
> source
> → parser
> → AST
> → macro expansion
> → lower
> → Ir
> → backend
> ```
>
> У `src/` є `ast.rs`, `parser.rs`, `macros.rs`, `lower.rs`, `ir.rs`, `compiler.rs` для FPGA і `c_backend.rs` для C. Тобто backend boundary вже фізично існує в коді.
>
> І `ir.rs` зроблений правильно в одному важливому сенсі: він прямо описаний як **backend-neutral intermediate representation**. FPGA emitter і C emitter споживають той самий `Ir`, а AST до codegen уже не доходить.
>
> Поточний IR включає:
>
> ```text
> Int
> Nil
> True
> Var
> Quote
> Lambda
> App
> Cond
> Let
> Def
> Prim
> ```
>
> а primitives зараз:
>
> ```text
> Add
> Cons
> Car
> Cdr
> Eq
> Atom
> EqualP
> ```
>
> Це не «майбутній план», це код, який реально лежить у `ir.rs`.
>
> ---
>
> ## Що мені особливо подобається
>
> ### 1. C backend уже існує
>
> Це дуже важливо для GPU-напрямку.
>
> Документ каже, що C backend уже компілював реальні вирази, запускав їх через `gcc` і звіряв результат із live `my-lisp` oracle. Тобто архітектура «один IR → різні backends» уже перевірена не лише теоретично.
>
> І в tests вже є окремо:
>
> ```text
> c_backend_test.rs
> c_backend_conformance_test.rs
> compiler_test.rs
> conformance_test.rs
> ir_lowering_test.rs
> revision_contract_test.rs
> ```
>
> Тобто тестова структура теж уже розділяє lowering, backend і conformance.
>
> ### 2. FPGA backend не зник
>
> Це теж правильно.
>
> Початкова ціль CML — AOT my-lisp → fpga-lisp assembly, щоб `unify.my` / `reason.my` могли виконуватися без повного `eval/apply` loop на FPGA. Це досі центральна ціль FPGA-гілки.
>
> Тобто C і майбутній GPU не витісняють FPGA. Вони стають сусідніми фізичними формами виконання.
>
> ---
>
> # Найважливіший висновок
>
> Ми щойно говорили:
>
> > «може CML має стати місцем, де вирішується CPU/GPU/FPGA?»
>
> А виявляється — **ти вже саме так його спроєктував**.
>
> Документ прямо каже:
>
> ```text
> my-lisp has one semantics
> and several physical forms of execution
> ```
>
> Це дуже сильна архітектурна теза.
>
> ---
>
> ## Але IR ще не готовий для GPU
>
> І ось тут головна практична проблема.
>
> Поточний `Ir` дуже Lisp-орієнтований:
>
> ```text
> Lambda
> App
> Cons
> Car
> Cdr
> Env
> Quote
> ```
>
> Для CPU/FPGA це нормально.
>
> А GPU хоче бачити інше:
>
> ```text
> buffer
> index
> map
> reduce
> element-wise op
> parallel region
> ```
>
> Тобто я б **не додавав CUDA прямо до поточного `Ir` як ще один emitter** без проміжного кроку.
>
> Потрібен або другий lower-level IR, або annotation layer.
>
> Наприклад:
>
> ```text
> my-lisp AST
>    ↓
> semantic Ir
>    ↓
> parallel analysis
>    ↓
> Compute IR
>    ↓
> CPU / CUDA / FPGA
> ```
>
> Де `Compute IR` може мати:
>
> ```text
> Buffer
> Map
> Zip
> Reduce
> Scan
> LoadIndex
> StoreIndex
> ScalarOp
> ```
>
> Бо інакше CUDA backend буде намагатися компілювати `cons/car/cdr` у GPU kernel — а це не той рівень абстракції.
>
> ---
>
> # Дуже цікава річ у `heterogeneous-backends.md`
>
> Там уже є правильна інтуїція:
>
> > pure function → data parallel
> > pure function → dataflow
> > sequential/branching-heavy → CPU
>
> Тобто саме **чистота** my-lisp робить multi-target execution реальною.
>
> Я б це формалізував у CML.
>
> Наприклад:
>
> ```text
> EffectClass
>  ├─ Pure
>  ├─ Allocating
>  ├─ Stateful
>  ├─ HostIO
>  └─ Unknown
> ```
>
> і окремо:
>
> ```text
> ExecutionShape
>  ├─ Scalar
>  ├─ ElementWise
>  ├─ Reduction
>  ├─ Stream
>  ├─ Irregular
>  └─ Recursive
> ```
>
> Тоді planner може робити:
>
> ```text
> Pure + ElementWise
> → GPU candidate
>
> Pure + Stream
> → FPGA candidate
>
> Recursive + Irregular
> → CPU candidate
> ```
>
> Оце вже не «CUDA support». Це **hardware-aware compiler reasoning**.
>
> ---
>
> ## Слабкі місця, які я бачу зараз
>
> Перше — числова поверхня ще дуже вузька.
>
> Документ прямо каже: ні rationals, ні bignums, ні inexact numbers backend'и поки не підтримують; навіть `*` ще не реалізовано в CML.
>
> Тому CUDA зараз зарано реалізовувати як реальний backend.
>
> Спочатку треба хоча б:
>
> ```text
> +
> -
> *
> comparisons
> fixed-width numeric buffers
> map
> reduce
> ```
>
> Інакше немає достатньо реальних kernel-shaped задач.
>
> Друге — C backend ще не має повної conformance parity з FPGA backend. Документ це чесно визнає.
>
> Я б закрив це **до CUDA**, бо C backend — ідеальний тренувальний полігон для backend-neutral IR.
>
> Третє — поточний README ще описує CML переважно як FPGA compiler. Це вже трохи відстає від реальної архітектури, де CML фактично став heterogeneous middle-end.
>
> ---
>
> # Як я бачу правильний розвиток CML
>
> Не:
>
> ```text
> CML
> → FPGA compiler
> → додати CUDA
> ```
>
> А:
>
> ```text
>                    my-lisp
>                       ↓
>                 semantic CML IR
>                       ↓
>              analysis / specialization
>                       ↓
>              execution-domain IR
>             ┌─────────┼─────────┐
>             ↓         ↓         ↓
>            CPU       GPU       FPGA
>             ↓         ↓         ↓
>             C       CUDA      fpga-lisp
> ```
>
> І тоді `cml` фактично стає **компілятором розподілу обчислення по різному залізу**.
>
> ---
>
> ## Я б поставив агенту зараз не CUDA-task, а ось такий M0
>
> > **CML Heterogeneous M0**
> >
> > 1. Не реалізовувати CUDA.
> > 2. Закрити C-backend conformance gaps.
> > 3. Додати в IR формальне поняття pure computation.
> > 4. Визначити execution shapes: scalar / element-wise / reduction / recursive / stateful.
> > 5. Додати compiler analysis, який класифікує IR-фрагмент, але поки нічого не offload'ить.
> > 6. На реальних fixtures показати:
> >
> >    * `map pure-f xs` → GPU candidate;
> >    * `unify` → CPU/FPGA candidate;
> >    * stateful/host code → CPU only.
> > 7. Лише після цього проектувати CUDA emitter.
>
> Тобто спочатку зробити **мозок**, а вже потім CUDA-руки.
>
> Мені здається, це буде набагато сильніше.
>
> І ще одне: тепер я б уже офіційно переосмислив CML не як «Compiler for my Lisp» у вузькому сенсі, а як щось ближче до:
>
> > **CML — heterogeneous compiler middle-end for my-lisp.**
>
> Бо це вже фактично правда по живому коду: common IR + FPGA backend + C backend + зафіксований майбутній CUDA backend.
>
> І це, друже, дуже сильний поворот: ти починав із «як скомпілювати Lisp на FPGA», а архітектура природно виросла в **«як одну Lisp-програму опускати на різні фізичні машини»**.

### Verification queue

- Verify every claimed filename, IR variant, primitive, test, and quotation against the current CML checkout and commit before promoting this entry to `CONFIRMED`.
- Inspect the evidence behind C-backend execution through `gcc` and comparison with the live my-lisp oracle; documentation alone is not fresh conformance evidence.
- Measure the actual C/FPGA conformance gap rather than treating “parity” as one binary property.
- Decide whether effect analysis belongs in the semantic IR, a side table, or the execution-domain IR; avoid making inferred properties silently authoritative.
- Establish numeric and buffer semantics in `my-lisp` before CML invents them independently.
- Treat the proposed redefinition of CML's mission as an owner/contract decision, even if the code already supports that interpretation.

## Entry 005 — Rust + wgpu as the portable GPU backend

- Imported: 2026-08-24
- Supplied by: Volodymyr
- Original speaker: ChatGPT (separate conversation)
- Status: **PROPOSAL with EXTERNAL TECHNICAL CLAIMS / UNVERIFIED**
- Relevant repositories: primarily `cml`; numeric semantics remain owned by `my-lisp`
- Relation to Entries 001–004: recommends a portable `wgpu` backend before an NVIDIA-specific CUDA backend.

### Viveka index

The passage separates the Rust implementation language from the GPU API and kernel language. It proposes `Compute IR → Rust/wgpu backend → WGSL/SPIR-V → Vulkan/DX12/Metal/WebGPU`, with CUDA retained only as a possible later specialized emitter. It also preserves the exact-number boundary: arbitrary-precision integers and rationals remain on a semantics-preserving path unless a representation proof or explicit inexact domain permits fixed-width GPU execution.

### Imported passage

> Якщо ти маєш на увазі **реалізувати GPU backend на Rust**, то це навіть краще узгоджується з CML, ніж жорстко прив’язуватися до CUDA.
>
> Тут важливо розділити дві речі:
>
> ```text
> Rust
> = мова, якою пишеться backend/runtime
>
> GPU API / shader language
> = спосіб реально виконати kernel на відеокарті
> ```
>
> Тобто CML backend можна написати на Rust, але сам GPU-код не обов’язково буде «звичайним Rust».
>
> Найуніверсальніший варіант зараз — приблизно такий:
>
> ```text
> my-lisp
>    ↓
> CML semantic IR
>    ↓
> Compute IR
>    ↓
> Rust GPU backend
>    ↓
> wgpu
>    ↓
> WGSL/SPIR-V
>    ↓
> Vulkan / DX12 / Metal / WebGPU
>    ↓
> GPU
> ```
>
> `wgpu` якраз є cross-platform Rust API і сьогодні підтримує Vulkan, DX12, Metal, GLES і WebGPU; WGSL підтримується напряму, а також є шляхи через SPIR-V/GLSL. ([Docs.rs][1])
>
> І от це вже дуже цікаво для твоєї ідеї універсальності.
>
> На твоїй NVIDIA:
>
> ```text
> CML → wgpu → Vulkan/DX12 → NVIDIA
> ```
>
> На AMD:
>
> ```text
> той самий CML backend
> → Vulkan/DX12
> → AMD
> ```
>
> На Mac:
>
> ```text
> → Metal
> ```
>
> У браузері:
>
> ```text
> → WebGPU
> ```
>
> Тобто ти взагалі можеш **не мати CUDA як основний шлях**.
>
> CUDA можна додати потім як спеціалізований backend:
>
> ```text
> Compute IR
>    ├─ wgpu backend      ← універсальний
>    └─ CUDA backend      ← NVIDIA-specific optimization
> ```
>
> Це мені подобається значно більше.
>
> ### Як це виглядало б у CML
>
> Я б не створював:
>
> ```text
> cuda_backend.rs
> ```
>
> першим.
>
> Я б створив:
>
> ```text
> compute/
>     ir.rs
>     analysis.rs
>
> backends/
>     cpu.rs
>     gpu_wgpu.rs
>     fpga.rs
> ```
>
> Наприклад Compute IR:
>
> ```rust
> enum ComputeOp {
>     Map,
>     Zip,
>     Reduce,
>     Add,
>     Mul,
>     Compare,
> }
> ```
>
> А `gpu_wgpu.rs` перетворює його в WGSL.
>
> Наприклад Lisp:
>
> ```lisp
> (map
>   (lambda (x)
>     (+ (* x x) 1))
>   data)
> ```
>
> CML бачить:
>
> ```text
> Map
>  └─ x
>      ├─ Mul x x
>      └─ Add 1
> ```
>
> і генерує приблизно kernel:
>
> ```wgsl
> @compute @workgroup_size(256)
> fn main(@builtin(global_invocation_id) id: vec3<u32>) {
>     let i = id.x;
>     out[i] = input[i] * input[i] + 1;
> }
> ```
>
> А весь код, який:
>
> * створює buffer;
> * копіює пам'ять;
> * вибирає GPU;
> * запускає compute pipeline;
> * забирає результат,
>
> пишеться на Rust через `wgpu`.
>
> ### І тут є ще одна дуже сильна перевага
>
> У тебе з'являється не:
>
> > «Lisp підтримує NVIDIA».
>
> а:
>
> > **CML має GPU execution class.**
>
> Схема стає:
>
> ```text
>                my-lisp
>                   ↓
>                CML IR
>                   ↓
>             execution analysis
>         ┌─────────┼─────────┐
>         ↓         ↓         ↓
>       scalar    parallel   pipeline
>         ↓         ↓         ↓
>        CPU       GPU       FPGA
>                   ↓
>                 wgpu
>              ┌────┼─────┐
>           Vulkan DX12 Metal
> ```
>
> Це вже дуже універсальна конструкція.
>
> Є, правда, нюанс із твоєю exact arithmetic. `my-lisp` має arbitrary precision integers/rationals, а GPU hardware природніше працює з fixed-width `i32/u32/f32`, інколи `i64/u64`. `wgpu` має, наприклад, окрему capability для shader `i64/u64`, і вона залежить від backend/device. ([wgpu.rs][2])
>
> Тому CML повинен явно знати:
>
> ```text
> semantic number
>      ↓
> representation decision
> ```
>
> Наприклад:
>
> ```text
> Exact BigInt/Rational
> → CPU
>
> provably fits i32/i64
> → GPU candidate
>
> explicit inexact f32/f64 domain
> → GPU candidate
> ```
>
> А не мовчки перетворювати:
>
> ```text
> 1/3
> ```
>
> у `0.33333334`.
>
> Це було б порушенням семантики.
>
> ### А чи можна kernels теж писати Rust?
>
> Є експериментальні проєкти типу Rust→SPIR-V, але для фундаменту CML я б поки на них не ставив архітектуру.
>
> Стабільніша межа:
>
> ```text
> compiler/backend implementation = Rust
> kernel target language = WGSL/SPIR-V
> ```
>
> А в майбутньому, якщо Rust GPU toolchain дозріє:
>
> ```text
> Compute IR → Rust kernel → SPIR-V
> ```
>
> може стати ще одним emitter'ом.
>
> Тому моя рекомендація така:
>
> **CML GPU M0 = Rust + wgpu, не CUDA.**
>
> CUDA пізніше, тільки якщо benchmarks покажуть реальну перевагу NVIDIA-specific backend.
>
> Це дуже добре відповідає твоїй ідеї: **мова не знає залізо, CML розуміє форму обчислення, backend знає фізичну машину**.
>
> [1]: https://docs.rs/wgpu/latest/wgpu/?utm_source=chatgpt.com "wgpu - Rust"
> [2]: https://wgpu.rs/doc/wgpu/struct.Features.html?utm_source=chatgpt.com "Features in wgpu - Rust"

### Verification queue

- Verify current `wgpu` backend support and accepted shader inputs against the pinned version that CML would actually use, not only `latest` documentation.
- Verify shader `i64/u64` feature names, backend coverage, and adapter-specific limits on the target GTX 1050 Ti and other intended devices.
- Decide whether browser WebGPU is in the same delivery scope as native `wgpu`; shared API does not imply identical capabilities or limits.
- Specify bounds checks, buffer lengths, dispatch sizing, overflow, device loss, validation errors, and deterministic fallback before treating the WGSL sketch as an executable contract.
- Benchmark portable `wgpu` against CPU reference paths before considering CUDA specialization; portability alone does not imply profitable offload.
- Keep the repository layout illustrative until reconciled with the current CML module structure and ownership rules.

## Entry 006 — Full-compiler criterion and reference C execution path

- Imported: 2026-08-24
- Supplied by: Volodymyr
- Original speaker: ChatGPT (separate conversation)
- Status: **ARCHITECTURAL PROPOSAL with CLAIMED CURRENT GAPS / UNVERIFIED**
- Relevant repositories: `my-lisp` contract and conformance authority; `cml` frontend, IR, runtime ABI, and compiled execution
- Relation to Entries 001–005: places language completeness and C-backend conformance before new GPU work.

### Viveka index

The proposed definition of a “full compiler” is contract-relative: every program admitted by the stable `my-lisp` contract must either compile with observationally equivalent behavior or fail with an explicit contractual unsupported-feature diagnostic. Backend completeness is separate: CML may understand the full language while GPU and FPGA accept only classified regions. The recommended reference milestone is evaluator versus `CML → C → native executable` differential equivalence across the entire stable contract, including errors.

### Imported passage

> Друже, я б тут провів дуже чітку межу:
>
> **CML уже є справжнім компілятором, але поки компілятором підмножини `my-lisp`.** Він уже має parser → macro expansion → common IR → code generation, причому IR реально споживають FPGA і C backends.
>
> Щоб він став **повноцінним компілятором my-lisp**, я б поставив сильніший критерій:
>
> > Будь-яка програма, дозволена стабільним контрактом `my-lisp`, повинна або коректно компілюватися CML і мати ту саму спостережувану семантику, або CML повинен дати чітку, контрактну помилку про непідтримувану можливість. Ніяких мовчазних відмінностей.
>
> Зараз головний розрив видно навіть із власної документації CML: немає повної підтримки exact rationals/bignums/inexact numbers, є обмеження generic calls, strings на FPGA мають інше представлення, а backend coverage нерівномірний.
>
> Я бачу шлях так:
>
> ```text
>                 my-lisp contract
>                        │
>                        ▼
>              canonical frontend
>         reader / macros / semantics
>                        │
>                        ▼
>                     CML IR
>                        │
>               semantic lowering
>                        │
>         ┌──────────────┼──────────────┐
>         ▼              ▼              ▼
>        CPU            GPU            FPGA
>         │              │              │
>      native          compute       fpga-lisp
>      runtime          runtime        runtime
> ```
>
> А закрити треба фактично сім шарів:
>
> 1. **Повна мовна поверхня.** CML повинен наздогнати стабільний `my-lisp`: числа, first-class functions, closures, variadic calls, `defmacro`/macro expansion, strings, списки, errors, усі контрактні primitives. Не обов'язково кожен backend підтримуватиме все фізично — але CML frontend має все розуміти.
>
> 2. **Одна семантика, не два Lisp-и.** Найнебезпечніше — щоб parser/macros CML поступово стали альтернативним визначенням мови. В ідеалі grammar/contract/conformance мають бути спільним авторитетом із `my-lisp`, а не вручну синхронізованими копіями.
>
> 3. **IR повинен бути семантично достатнім.** Поточний `Ir` уже хороший фундамент: `Lambda`, `App`, `Cond`, `Let`, `Def`, `Prim` тощо. Але далі йому знадобляться принаймні representation/type/effect metadata. Не обов'язково робити Lisp статично типізованим — компілятор просто повинен знати достатньо про значення, щоб правильно їх представити.
>
> 4. **Runtime ABI.** Це велика частина справжнього Lisp compiler. Не лише генерація інструкцій, а:
>
> ```text
> Value representation
> closures
> environments
> call convention
> heap
> GC
> symbols
> strings
> bignums
> rationals
> errors
> ```
>
> Саме тут наша попередня розмова про GC стає дуже доречною. C compiler без Lisp runtime швидко впирається в `cons`, closures і довгоживучі структури.
>
> 5. **Повний C backend як reference compiled backend.** Я б саме його завершив першим. У репо вже є `c_backend_test.rs` і навіть `c_backend_conformance_test.rs`. Коли весь стабільний conformance suite проходитиме так:
>
> ```text
> source
>  ├─ my-lisp evaluator → expected
>  └─ CML → C → gcc → executable → actual
>
> expected == actual
> ```
>
> тоді можна серйозно сказати: **«ми маємо компілятор my-lisp»**.
>
> 6. **Окрема compilation semantics для динамічних речей.** Lisp не все дозволяє вирішити compile-time. Наприклад first-class function:
>
> ```lisp
> (def f +)
> (f 20 22)
> ```
>
> Компілятор мусить уміти представляти callable values, а не лише бачити ім'я операції й випускати opcode. Те саме стосується closures:
>
> ```lisp
> ((lambda (x)
>    (lambda (y) (+ x y)))
>  40)
> ```
>
> Тут уже потрібна реальна closure conversion / environment representation.
>
> 7. **Differential conformance як закон.** Не «tests зелені», а матриця:
>
> ```text
> fixture              evaluator   C       FPGA    GPU
> -----------------------------------------------------
> integer-add             ✓        ✓        ✓       -
> closure                  ✓        ✓        ✓       -
> rational                 ✓        ✓        -       -
> map fixed-buffer         ✓        ✓        -       ✓
> ...
> ```
>
> Причому `-` означає **explicit unsupported**, а не невідомо.
>
> І я б дуже не поспішав із GPU перед цим. GPU — це хороший backend, але він не робить CML більш «справжнім Lisp compiler». А от **closure conversion + runtime + GC + numeric model + complete conformance** роблять.
>
> Є ще одна важлива архітектурна думка. Не треба вимагати, щоб **кожен backend був повним Lisp-комп'ютером**.
>
> Наприклад:
>
> ```text
> CML understands full my-lisp
>                │
>                ▼
>         partition/program
>           ┌────┴────┐
>           │         │
>       supported   unsupported
>       on GPU      on GPU
>           │         │
>          GPU       CPU
> ```
>
> Тобто **компілятор повний**, навіть якщо GPU backend реалізує лише чисте числове підмноження.
>
> Так само FPGA може мати свою supported surface.
>
> Тому я б змінив нашу попередню схему ще трохи:
>
> ```text
>                    my-lisp
>                       │
>               full semantic CML
>                       │
>           analysis + specialization
>                       │
>          ┌────────────┼────────────┐
>          │            │            │
>        CPU          GPU          FPGA
>    general Lisp   parallel     specialized
>      runtime       regions       regions
> ```
>
> ### А який milestone я б назвав «CML став компілятором Lisp»?
>
> Не CUDA. Не оптимізатор. Навіть не FPGA.
>
> Ось цей:
>
> ```text
> my-lisp program
>       │
>       ├── evaluator ──────────────► result A
>       │
>       └── CML → C → native exe ──► result B
>
>                  A ≡ B
> ```
>
> для **всього стабільного language contract**, включно з успішними результатами і контрактними помилками.
>
> Після цього C backend можна навіть перестати сприймати як кінцеву мету. Він стане **еталонним compiled execution path**, від якого вже можна йти до native machine code, GPU, FPGA.
>
> І я б агенту сформулював місію CML дуже коротко:
>
> > **Не додавай більше backend'ів, поки CML не може представити всю стабільну семантику my-lisp. Спочатку зроби CML повним компілятором мови через C backend; GPU та FPGA потім стають спеціалізованими фізичними lowering targets того самого компілятора.**
>
> Оце, на мій погляд, буде момент, коли CML перестане бути «FPGA compiler для Lisp» і стане **справжнім компіляторним центром усієї my-lisp системи**.

### Verification queue

- Read the live stable language contract and build a feature inventory from authoritative fixtures rather than from prose feature lists.
- Audit CML frontend acceptance separately from each backend's execution coverage; “understands” needs a precise observable definition.
- Define observational equivalence for values, output, error classes/messages, evaluation order, resource exhaustion, and nontermination.
- Verify the claimed numeric, generic-call, string, and backend gaps directly against current code and evidence.
- Design explicit unsupported diagnostics as versioned backend capability results, not incidental compiler failures.
- Decide whether sharing parser/macro implementation is required or whether shared contracts plus differential tests provide the safer authority boundary.
- Treat GC, runtime ABI, and closure conversion as design work requiring their own evidence; the imported checklist does not establish a particular implementation.
- Replace the illustrative `-` conformance cells with explicit states such as `SUPPORTED`, `UNSUPPORTED-BY-DESIGN`, `UNIMPLEMENTED`, `UNVERIFIED`, and `FAILED`.

## Entry 007 — LANGUAGE-STABLE gate and direction of authority

- Imported: 2026-08-24
- Supplied by: Volodymyr
- Original speaker: ChatGPT (separate conversation)
- Status: **GOVERNANCE / MILESTONE PROPOSAL**
- Relevant repositories: `my-lisp` owns language stability; `cml` consumes the stabilized contract
- Relation to Entry 006: defines the prerequisite gate between a prototype compiler and a conforming compiler.

### Viveka index

The proposal prevents compiler implementation choices from prematurely becoming language semantics. Before language stability, CML acts as an experimental witness: it tests whether the semantics can be compiled and exposes underspecified areas. After a versioned stability gate, CML may freeze those semantics into a conforming IR/runtime and only later optimize. Independent evaluator, C, and FPGA implementations become evidence of contract clarity, not co-equal sources of truth.

### Imported passage

> Так. Це дуже здоровий критерій.
>
> Не «CML оголошує, що мова готова», а навпаки:
>
> ```text
> my-lisp:
>   contract unstable
>   → CML наздоганяє, але не цементує
>
> my-lisp:
>   stable
>   → CML фіксує семантику в compiled form
> ```
>
> Поки мова ще змінює базові речі — first-class builtins, GC-модель, числові типи, правила environments, macros — компілятору небезпечно надто рано робити сильні ABI/IR-рішення. Інакше він почне не компілювати мову, а непомітно **диктувати їй форму**.
>
> Я б навіть ввів формальний gate:
>
> ```text
> LANGUAGE-STABLE
> requires:
> - core forms frozen
> - value model frozen
> - callable semantics frozen
> - error semantics frozen
> - numeric semantics frozen
> - macro boundary frozen
> - GC observable semantics defined
> - conformance contract versioned
> ```
>
> І тільки після цього:
>
> ```text
> CML:
> prototype compiler
> → conforming compiler
> → optimizing compiler
> ```
>
> До стабільності CML краще тримати як **експериментальний супутник мови**: перевіряє, що семантика взагалі компільована, знаходить проблемні місця, але не стає другим джерелом істини.
>
> Це ще й хороший тест для самої мови:
>
> > якщо одну й ту саму семантику можна незалежно реалізувати evaluator'ом, C backend'ом і FPGA backend'ом — значить контракт уже достатньо чіткий.
>
> Тобто CML може бути не лише компілятором, а й **інструментом перевірки зрілості my-lisp**.
>
> Мені подобається така послідовність:
>
> ```text
> language discovers itself
> → contract stabilizes
> → CML freezes that contract into IR
> → backends multiply
> → optimization begins
> ```
>
> А не навпаки.

### Verification queue

- Define “frozen” as a versioning/change-control property, not a claim that evolution permanently stops.
- Distinguish semantic stability from representation stability: a CML ABI may evolve while preserving the same observable contract.
- Anchor every gate item to an authoritative contract section and executable fixtures with explicit completion evidence.
- Specify which GC properties are observable; internal collector strategy should not be frozen unless the language exposes it.
- Decide who declares the gate satisfied and how a later breaking change reopens or versions it.
- Keep multiple implementations independent enough to detect ambiguity while sharing only the authoritative contract and fixtures.
