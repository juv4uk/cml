# Third-party dependencies

This repository remains MIT licensed. Cargo dependencies retain their own
licenses; no dependency source is copied into this repository.

| Dependency | Version policy | License | Purpose | Source |
|---|---:|---|---|---|
| `wgpu` | `30.0.0` | MIT OR Apache-2.0 | Optional `gpu-wgpu` device/runtime backend | <https://crates.io/crates/wgpu/30.0.0> |
| `pollster` | `1.0.1` | MIT OR Apache-2.0 | Blocking native entry point for the optional runtime | <https://crates.io/crates/pollster/1.0.1> |
| `cudarc` | `0.19.9` | MIT OR Apache-2.0 | Optional CUDA Driver and NVRTC runtime backend | <https://crates.io/crates/cudarc/0.19.9> |

The optional dependency was introduced only after checking the ecosystem
license matrix. Both offered licenses are permissive and compatible with this
repository's MIT license.

---

# Сторонні залежності (Ukrainian)

Цей репозиторій залишається під ліцензією MIT. Залежності Cargo зберігають 
власні ліцензії; вихідний код жодної із залежностей не копіюється до цього 
репозиторію.

| Залежність | Політика версій | Ліцензія | Призначення | Джерело |
|---|---:|---|---|---|
| `wgpu` | `30.0.0` | MIT OR Apache-2.0 | Опціональний `gpu-wgpu` пристрій/середовище виконання | <https://crates.io/crates/wgpu/30.0.0> |
| `pollster` | `1.0.1` | MIT OR Apache-2.0 | Синхронна точка входу для опціонального середовища виконання | <https://crates.io/crates/pollster/1.0.1> |
| `cudarc` | `0.19.9` | MIT OR Apache-2.0 | Опціональний бекенд для виконання через CUDA Driver та NVRTC | <https://crates.io/crates/cudarc/0.19.9> |

Опціональна залежність була додана лише після перевірки матриці ліцензій 
екосистеми. Обидві запропоновані ліцензії є дозволяючими (permissive) та 
сумісними з ліцензією MIT цього репозиторію.
