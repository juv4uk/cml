# DOC-SURVEY-2026-09-01 — огляд нових доків cml

**Виконавець:** wsl-nidana-1 (ecosystem-координаційна сесія)
**Метод:** `git log --since=2026-08-28 --oneline` + читання субстантивних
доків. Виключено вже відоме: compatibility.my/compute-contract.my статус,
conformance-пайплайн (tests/conformance_test.rs), semantic-gate
(src/semantic.rs, src/compute.rs).

## Компілятор реально виріс до п'яти бекендів

- **`docs/heterogeneous-backends.md`** (секція "Current Executable State —
  August 2026"): FPGA (hardware-verified), C (oracle-verified), x86_64
  Freestanding (`src/x86_freestanding.rs`, годує wsm-os), **CUDA**
  (`src/gpu_cuda.rs`/`gpu_cuda_runtime.rs`, PTX через NVRTC), WGPU (портативний
  WGSL, опціональний). `Ir` тепер покриває Compute-вузли (`Map`/`Reduce`/
  `Scan`/`Index`/`ParallelRegion`) + `Buffer(I32/F32)` + `TailSelfCall`. Усі
  п'ять судяться проти того самого `language-contract.my`/`isa-contract.my`/
  `compatibility.my`, не одне проти одного.

## Реальний GPU-доказ на залізі

- **`docs/cuda-runtime.md`** — датовано 2026-08-24: власна `gpu-cuda`-фіча
  CML виконала допущений i32 `map` на реальній NVIDIA GTX 1050 Ti (compute
  cap 6.1, через WSL-міст `libcuda.so` — нативний Linux-драйвер у цьому
  середовищі повертає `CUDA_ERROR_NO_DEVICE`), результат `[2,3,4]`. Явно
  обмежено: доводить лише цей один зріз — не auto-offload, не f32, не
  ROCm/oneAPI.

  **Корекція межі wsm-cuda/wsm-os, обговореної раніше цієї сесії**: CML вже
  має робочий *hosted* CUDA-бекенд, незалежний від окремого репо `wsm-cuda`.
  Тобто тепер існують ДВІ hosted-CUDA поверхні, чиє співвідношення ніде не
  задокументовано. Bare-metal-розрив (G1-G5, без vendor-драйвера) лишається
  недоторканим обома — шлях CML йде через NVRTC/host-драйвер, так само як
  заявлений scope wsm-cuda. (Ця корекція вже внесена у `wsm-os/docs/VISION.md`.)

## Дисципліна доказів у дії

- **`docs/full-semantic-ir-independent-verification-2026-08-30.md`** —
  незалежна перевірка коміту `ce2d393` ("semantic debt" fix). Вердикт:
  реальний, але частковий — фікс лаверингу рядків і error-path
  C-бекенду для quoted-значень підтверджені (12/12 targeted тести), але
  `validate_ir` FPGA досі допускає `Ir::Builtin`, який emission не вміє
  обробити — тому ширше твердження "жоден бекенд не панікує" явно позначено
  unverified. Гарний приклад того, як власна дисципліна доказів ловить
  overclaim.

- **`docs/testing.md`** (тепер двомовний): сліпий pipeline
  parse→macro-expand→compile→assemble→simulate→decode→compare незмінний, але
  реальні числа з `fail-closed-conformance-report-2026-08-27.md`: Tier-1 FPGA
  E2E = 2/2 pass; **Tier 2 (105 фікстур) і Tier 3 (84) = нуль виконано**.

## Джерело роботи, зробленої раніше цієї сесії

Свіжі x86-коміти (`5b31189` closure-convert, `7c3ef66` lexical frame,
`8232c33` PCI capability admission) підтверджують: саме x86_freestanding-
бекенд CML, не сам wsm-os, реально емітує асемблер для closure-runtime і
PCI-capability роботи — wsm-os споживає вихід CML, не генерує його сам.

Немає окремого `ADR/`-каталогу в cml — архітектурні рішення живуть у доках
вище, не в окремих ADR-файлах.
