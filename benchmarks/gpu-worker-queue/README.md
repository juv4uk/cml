# Вимірювання черги спільного CUDA worker

Слайс #476 додає лише opt-in timing evidence до worker protocol v1.

Увімкнення:
```sh
CML_GPU_WORKER_TIMING=1 cml-gpu-worker serve
```

Клієнтський виклик у тому ж режимі:
```sh
CML_GPU_WORKER_TIMING=1 cml-gpu-worker chain-file-i32 input.bin output.bin 1 2 3
```

Вимірювання:
- `client_total_ns` — повний монотонний client round-trip.
- `service_ns` — монотонний server-side час від початку обробки accepted request до підготовки response body; включає decode, file I/O та CUDA execution.
- `cuda_ns` — існуючий вимір навколо CUDA chain execution.
- `non_service_ns = client_total_ns - service_ns` — час поза server service: accept/connection, serialized wait, transport та інший overhead. Без окремого queue timestamp це не називається чистим queue wait.

У штатному режимі без `CML_GPU_WORKER_TIMING=1` вихід і протокол не змінюються.

Timing є diagnostic evidence і не впливає на semantic result, worker serialization або scheduling policy.
