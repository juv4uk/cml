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
