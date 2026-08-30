# Intel GPU runtime boundary

CML represents Intel as a vendor-neutral accelerator candidate with two
possible execution paths:

```text
portable     → wgpu
vendor path  → oneAPI Level Zero (planned)
```

Backend availability is established by live discovery, not inferred from a
vendor name. `BackendCapability` records `Live`, `Planned`, or `Unsupported`;
only `Live` can become an `AcceleratorDescriptor` consumed by the planner.

## Current WSL host evidence (2026-08-24)

Windows reports an Intel HD Graphics 530 (Skylake / Gen9, PCI `8086:1912`).
Intel's compute-runtime legacy-platform matrix supports Skylake through the
`legacy1` packages for native Linux OpenCL 3.0 and Level Zero 1.5, but marks
WSL support as unavailable. Therefore this host is recorded as:

```text
Intel HD Graphics 530 + native Linux + legacy1 → possible, not tested here
Intel HD Graphics 530 + WSL2                   → unsupported by upstream matrix
NVIDIA GTX 1050 Ti + WSL2 + CUDA               → live-tested CML path
```

This is a host/platform limitation, not a reason to remove Intel from CML's
cross-vendor model. A future Intel device discovered through wgpu or Level
Zero may enter the same selection policy without changing my-lisp semantics or
Compute IR.

Primary upstream reference:
<https://github.com/intel/compute-runtime/blob/master/documentation/LEGACY_PLATFORMS.md>

---

# Межа середовища виконання Intel GPU (Ukrainian)

CML розглядає Intel як вендор-незалежного (vendor-neutral) кандидата на роль 
прискорювача з двома можливими шляхами виконання:

```text
портативний      → wgpu
шлях вендора     → oneAPI Level Zero (заплановано)
```

Наявність бекенда визначається шляхом живого виявлення (live discovery), а не
виводиться з назви вендора. `BackendCapability` записує `Live`, `Planned` або 
`Unsupported`; лише `Live` може стати дескриптором прискорювача 
(`AcceleratorDescriptor`), який використовується планувальником.

## Поточні докази хоста WSL (2026-08-24)

Windows повідомляє про наявність Intel HD Graphics 530 (Skylake / Gen9, 
PCI `8086:1912`). Матриця застарілих платформ (legacy-platform matrix) середовища 
виконання compute-runtime від Intel підтримує Skylake через пакети `legacy1` 
для нативного Linux OpenCL 3.0 та Level Zero 1.5, але позначає підтримку WSL як 
недоступну. Тому цей хост записано так:

```text
Intel HD Graphics 530 + нативний Linux + legacy1 → можливо, тут не перевірялося
Intel HD Graphics 530 + WSL2                     → не підтримується апстрім-матрицею
NVIDIA GTX 1050 Ti + WSL2 + CUDA                 → перевірений наживо шлях CML
```

Це обмеження хоста/платформи, а не причина видаляти Intel із міжвендорної 
(cross-vendor) моделі CML. Майбутній пристрій Intel, виявлений через wgpu 
або Level Zero, зможе увійти в ту саму політику вибору без змін у семантиці 
`my-lisp` або Compute IR.

Головне посилання на джерело (upstream):
<https://github.com/intel/compute-runtime/blob/master/documentation/LEGACY_PLATFORMS.md>
