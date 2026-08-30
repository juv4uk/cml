# CPU + GPU + FPGA execution fabric

**Status:** architecture direction, 2026-08-24. Machine-readable contracts
remain authoritative.

CML is the integration point of one semantic system, not a second Lisp:

```text
my-lisp semantics -> CML semantic IR -> Execution Graph
                                          |    |    |
                                         CPU  GPU  FPGA
```

The CPU owns full semantics and fallback. GPU executors own pure bulk work on
immutable typed numeric buffers. The portable direction is Rust + `wgpu`;
CUDA is an optional NVIDIA optimization, while future Intel Level Zero/oneAPI
and AMD ROCm implementations enter through the same capability contract. The
FPGA executes either general Lisp through `fpga-lisp` ISA or, later, proven
stream/dataflow regions through a separate specialization path.

## Execution Graph boundary

CML should represent explicit nodes, dependencies, logical buffers, and
targets:

```rust
enum ExecutionTarget {
    Cpu,
    Gpu { backend: GpuBackend },
    Fpga { device: String },
}

struct PlanNode {
    id: NodeId,
    operation: Operation,
    inputs: Vec<BufferId>,
    outputs: Vec<BufferId>,
    dependencies: Vec<NodeId>,
    target: ExecutionTarget,
}
```

Raw host/device pointers never cross this boundary. M0 moves values through
host-visible immutable buffers; zero-copy and direct transfers are later,
semantics-preserving optimizations.

Every backend reports capabilities and implements prepare/execute. Only
`Live` capabilities are selectable. `Planned`, unknown representation facts,
unsupported effects, or unsatisfied numeric obligations reject acceleration.
Outputs become visible atomically; failures expose no partial language value.

Planner order is:

```text
semantic safety
-> live capability
-> supported representation and operation
-> transfer/launch cost and workload size
-> selected target
```

CPU is always the canonical fallback. Initial placement is explicit and
deterministic; automatic placement waits for differential correctness and
measurements.

## Milestones

1. **M0:** graph plus CPU executor; multi-node dependency, buffer, and failure
   tests; GPU/FPGA targets fail closed without registered live executors.
2. **M1:** lower `numeric-buffer-map`; compare CPU with one live GPU executor;
   retain CUDA and portable `wgpu` behind the same interface.
3. **M2:** versioned FPGA job/result frames and Rust transport; execute a graph
   node on the physical board while retaining the monitor as an independent
   diagnostic tool.
4. **M3:** golden CPU -> GPU -> CPU -> FPGA -> CPU pipeline, checked against
   reference execution. This proves orchestration, not speedup.
5. **M4:** measured, explainable, cost-aware placement and safe fallback.

Conformance records `CONFIRMED`, `PARTIAL`, `UNSUPPORTED`, `UNAVAILABLE`,
`BROKEN`, or `UNRESOLVED` per backend. `i32` values and overflow behavior must
match; exact values are never silently converted to `f32`; floating comparison
rules belong in a named contract.

The first code slice is M0 in CML. It gives the existing CPU and CUDA paths,
the portable GPU direction, and the connected FPGA one stable seam without
putting hardware names into my-lisp semantics.

---

# Тканина виконання CPU + GPU + FPGA (Ukrainian)

**Статус:** архітектурний напрямок, 2026-08-24. Машиночитні контракти
залишаються авторитетними.

CML є точкою інтеграції єдиної семантичної системи, а не другим Lisp-ом:

```text
семантика my-lisp -> семантичний IR CML -> Граф виконання
                                            |    |    |
                                           CPU  GPU  FPGA
```

CPU відповідає за повну семантику та відкат (fallback). Виконавці GPU 
відповідають за чисті об'ємні обчислення над незмінними (immutable) 
типізованими числовими буферами. Портативний напрямок — це Rust + `wgpu`; 
CUDA є опціональною оптимізацією для NVIDIA, тоді як майбутні реалізації 
Intel Level Zero/oneAPI та AMD ROCm інтегруються через той самий контракт 
можливостей. FPGA виконує або загальний Lisp через архітектуру команд (ISA) 
`fpga-lisp`, або, пізніше, доведені регіони потоків/потоку даних 
(stream/dataflow regions) через окремий шлях спеціалізації.

## Межа графу виконання (Execution Graph boundary)

CML повинен представляти явні вузли, залежності, логічні буфери та цілі:

```rust
enum ExecutionTarget {
    Cpu,
    Gpu { backend: GpuBackend },
    Fpga { device: String },
}

struct PlanNode {
    id: NodeId,
    operation: Operation,
    inputs: Vec<BufferId>,
    outputs: Vec<BufferId>,
    dependencies: Vec<NodeId>,
    target: ExecutionTarget,
}
```

Сирі (raw) вказівники хоста/пристрою ніколи не перетинають цю межу. M0 
переміщує значення через видимі для хоста незмінні буфери; zero-copy 
(нульове копіювання) та прямі передачі — це пізніші оптимізації, які 
зберігають семантику.

Кожен бекенд повідомляє про свої можливості та реалізує підготовку/виконання 
(prepare/execute). Доступні для вибору лише `Live` можливості. `Planned` 
(заплановані), невідомі факти представлення, непідтримувані ефекти або 
незадоволені числові зобов'язання відхиляють акселерацію. Результати стають 
видимими атомарно; збої не залишають частково змінених значень мови.

Порядок планувальника:

```text
семантична безпека
-> наявна (live) можливість
-> підтримуване представлення та операція
-> вартість передачі/запуску та розмір навантаження
-> обрана ціль (target)
```

CPU завжди є канонічним відкатом (fallback). Початкове розміщення є явним 
та детермінованим; автоматичне розміщення чекає на диференціальну 
правильність та вимірювання.

## Етапи (Milestones)

1. **M0:** граф плюс CPU-виконавець; тести з багатьма вузлами, залежностями, 
   буферами та збоями; GPU/FPGA цілі відхиляються (fail closed) без 
   зареєстрованих живих виконавців.
2. **M1:** lowering для `numeric-buffer-map`; порівняння CPU з одним живим 
   GPU-виконавцем; збереження CUDA та портативного `wgpu` за тим самим 
   інтерфейсом.
3. **M2:** версіоновані кадри завдань/результатів (job/result frames) FPGA 
   та транспорт на Rust; виконання вузла графа на фізичній платі із 
   збереженням монітора як незалежного діагностичного інструменту.
4. **M3:** еталонний конвеєр CPU -> GPU -> CPU -> FPGA -> CPU, перевірений 
   відносно референсного виконання. Це доводить правильність оркестрації, 
   а не прискорення.
5. **M4:** вимірюване, зрозуміле розміщення з урахуванням вартості та 
   безпечний відкат.

Сумісність записує `CONFIRMED`, `PARTIAL`, `UNSUPPORTED`, `UNAVAILABLE`, 
`BROKEN` або `UNRESOLVED` для кожного бекенда. Значення `i32` та поведінка 
при переповненні мають збігатися; точні значення ніколи не перетворюються 
мовчки на `f32`; правила порівняння чисел із рухомою комою належать до 
іменованого контракту.

Перший зріз коду — M0 у CML. Він надає існуючим шляхам CPU та CUDA, 
портативному GPU напрямку та підключеному FPGA один стабільний шов, 
не вписуючи назви апаратного забезпечення в семантику my-lisp.
