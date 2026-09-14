# CML Native Performance Baseline (#53) / Еталонний нативний стенд продуктивності

## Українська (Ukrainian)

### Мета та концепція

Цей стенд встановлює **відтворюваний базовий вимір продуктивності та структурної якості коду (code quality baseline)** для CML перед впровадженням оптимізаційних проходів дорожньої карти (#54 Lowered CFG, #55 Numeric Unboxing, #58 Inlining, #57 DCE, #56 RegAlloc, #59 AVX2).

Головні принципи:
1. **Роздільні вердикти коректності та швидкодії**: швидший код, що дає неправильний результат, є дефектом, а не оптимізацією.
2. **Семантичний авторитет Lisp**: очікуваний числовий результат ніколи не дублюється в Rust «магічними константами» — він походить від Lisp-свідка / оракула.
3. **Структурні метрики (детерміновані)**:
   - Розмір машинного коду в байтах (`code_bytes`).
   - Кількість інструкцій (`instruction_count`).
   - Кількість звернень до пам'яті (`load_count`, `store_count`).
   - Кількість переходів і викликів (`branch_count`, `call_count`).
   - Спіли регістрів (`spill_count`).
4. **Динамічні метрики (Skylake i5-6400)**:
   - Розділення прогріву (`warmup`) та вимірювань (`measured`).
   - Мінімум, медіана та максимум у наносекундах (`min_ns`, `median_ns`, `max_ns`).
   - Чітка прив'язка до профілю процесора (`target_cpu_profile`).

### Атестований набір навантажень (Corpus)

1. **`scalar-add`**: скалярне 64-бітне додавання `(+ 10 32) -> 42`.
2. **`counted-loop-sum-1000`**: числовий цикл зі зворотним відліком від 1000 до 0 з акумуляцією суми `-> 500500`.
3. **`buffer-map-i32`**: неперервний буферний map `(+ x 1)` над `#i32(1 2 3 4 5)` з Compute IR `-> #i32(2 3 4 5 6)`.
4. **`unsupported-dynamic-fail-closed`**: динамічна операція поза атестованою межею, що перевіряє відмову fail-closed.

### Як запустити та оновити артефакт

```bash
# Генерація повного звіту у JSON:
cargo run --bin cml-baseline -- --out benchmarks/baseline-skylake.json

# Вивід у форматі S-виразів (S-expressions):
cargo run --bin cml-baseline -- --sexpr

# Перевірка детермінізму та тестів стенду:
cargo test --test native_perf_baseline_test
```

---

## English

### Purpose & Concept

This suite establishes a **reproducible code-quality and runtime performance baseline** for CML prior to introducing the Native Performance Roadmap optimizations (#54 Lowered CFG, #55 Numeric Unboxing, #58 Inlining, #57 DCE, #56 RegAlloc, #59 AVX2).

Key invariants:
1. **Separate verdicts for correctness and speed**: a faster execution that computes an incorrect value is a defect, never an optimization.
2. **Lisp semantic authority**: expected outcomes are derived from the Lisp oracle, never hardcoded as magic numbers.
3. **Deterministic structural metrics**: code bytes, instruction count, loads, stores, branches, calls, and register spills.
4. **Dynamic microbenchmarking**: warm-up separated from measurement, reporting min, median, and max nanoseconds keyed by target CPU profile.

### Workload Corpus

1. **`scalar-add`**: scalar 64-bit integer addition `(+ 10 32) -> 42`.
2. **`counted-loop-sum-1000`**: numeric countdown loop 1000 down to 0 accumulating sum `-> 500500`.
3. **`buffer-map-i32`**: contiguous i32 buffer map `(+ x 1)` from Compute IR `-> #i32(2 3 4 5 6)`.
4. **`unsupported-dynamic-fail-closed`**: unadmitted dynamic operation verifying fail-closed policy.

### Execution & Reproduction

```bash
# Generate JSON baseline artifact:
cargo run --bin cml-baseline -- --out benchmarks/baseline-skylake.json

# Output as Lisp S-expressions:
cargo run --bin cml-baseline -- --sexpr

# Verify determinism and correctness via tests:
cargo test --test native_perf_baseline_test
```
