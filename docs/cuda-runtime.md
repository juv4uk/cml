# CUDA runtime

CML's optional `gpu-cuda` feature executes semantically admitted map regions
through one shared path:

```text
my-lisp source → CML IR → compute admission → CUDA C → NVRTC → PTX
→ CUDA Driver launch → readback
```

It is an NVIDIA-optimized sibling of the portable `gpu-wgpu` path, not part of
my-lisp semantics and not a replacement for AMD/Intel portability.

## WSL driver-library precedence

This machine has two different `libcuda.so` implementations installed:

- `/usr/lib/wsl/lib/libcuda.so` — the working WSL GPU bridge;
- `/lib/x86_64-linux-gnu/libcuda.so` — a native Linux NVIDIA driver library
  which loads successfully but reports `CUDA_ERROR_NO_DEVICE` under WSL.

Because `cudarc` dynamically opens `libcuda.so`, live tests must give the WSL
bridge precedence:

```sh
LD_LIBRARY_PATH=/usr/lib/wsl/lib:/lib/x86_64-linux-gnu \
  cargo test --features gpu-cuda --test gpu_cuda_live_test -- --ignored --nocapture
```

Evidence recorded 2026-08-24: the admitted i32 map executed on NVIDIA GeForce
GTX 1050 Ti device 0 and returned `[2, 3, 4]`. Live discovery reported compute
capability 6.1 and 4,294,705,152 bytes, produced an NVIDIA/CUDA/discrete-GPU
planner descriptor, and `VendorOptimized` selected it. This proves the CML CUDA
path for that slice only; it does not prove automatic offload, the f32 path,
ROCm, or oneAPI Level Zero.
