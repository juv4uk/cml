# Third-party dependencies

This repository remains MIT licensed. Cargo dependencies retain their own
licenses; no dependency source is copied into this repository.

| Dependency | Version policy | License | Purpose | Source |
|---|---:|---|---|---|
| `wgpu` | `30.0.0` | MIT OR Apache-2.0 | Optional `gpu-wgpu` device/runtime backend | <https://crates.io/crates/wgpu/30.0.0> |
| `pollster` | `1.0.1` | MIT OR Apache-2.0 | Blocking native entry point for the optional runtime | <https://crates.io/crates/pollster/1.0.1> |

The optional dependency was introduced only after checking the ecosystem
license matrix. Both offered licenses are permissive and compatible with this
repository's MIT license.
