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

---

# CUDA середовище виконання (Ukrainian)

Опціональна функція (feature) `gpu-cuda` у CML виконує семантично допущені 
(admitted) регіони `map` через один спільний шлях:

```text
джерело my-lisp → CML IR → допуск до compute → CUDA C → NVRTC → PTX
→ запуск CUDA Driver → зворотне зчитування (readback)
```

Це оптимізований для NVIDIA брат портативного шляху `gpu-wgpu`, який не є
частиною семантики `my-lisp` і не замінює портативність для AMD/Intel.

## Пріоритет драйверних бібліотек у WSL

На цій машині встановлено дві різні реалізації `libcuda.so`:

- `/usr/lib/wsl/lib/libcuda.so` — працюючий міст WSL GPU;
- `/lib/x86_64-linux-gnu/libcuda.so` — нативна бібліотека драйвера NVIDIA 
  для Linux, яка успішно завантажується, але повідомляє про 
  `CUDA_ERROR_NO_DEVICE` в середовищі WSL.

Оскільки `cudarc` динамічно відкриває `libcuda.so`, живі тести повинні 
надавати пріоритет мосту WSL:

```sh
LD_LIBRARY_PATH=/usr/lib/wsl/lib:/lib/x86_64-linux-gnu \
  cargo test --features gpu-cuda --test gpu_cuda_live_test -- --ignored --nocapture
```

Докази, записані 2026-08-24: допущений `map` i32 виконався на пристрої 
NVIDIA GeForce GTX 1050 Ti (пристрій 0) і повернув `[2, 3, 4]`. Живе 
виявлення (live discovery) повідомило про compute capability 6.1 та 
4,294,705,152 байт пам'яті, створило дескриптор планувальника 
NVIDIA/CUDA/discrete-GPU, і стратегія `VendorOptimized` обрала його. 
Це доводить працездатність шляху CUDA у CML лише для цього зрізу (slice); 
це не доводить роботу автоматичного вивантаження (offload), шляху f32, 
ROCm або oneAPI Level Zero.
