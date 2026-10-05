# Вимірювання черги спільного CUDA worker

Слайс #476 додає лише opt-in timing evidence до вже існуючого протоколу worker v1.

Увімкнення:

```sh
CML_GPU_WORKER_TIMING=1 cml-gpu-worker serve
```

Той самий режим на клієнті:

```sh
CML_GPU_WORKER_TIMING=1 cml-gpu-worker chain-file-i32 input.bin output.bin 1 2 3
```

У timing-режимі клієнт друкує `client_total_ns=...`; worker додає `service_ns=...` до відповіді.

Визначення:

- `client_total_ns` — повний монотонний час одного клієнтського round-trip.
- `service_ns` — монотонний час від початку server-side обробки accepted request до завершення підготовки response body; він включає decode, input/output file I/O та CUDA execution.
- `cuda_ns` — вже наявний вимір безпосередньо навколо CUDA chain execution.
- `non_service_ns = client_total_ns - service_ns` — різниця поза server service. Вона може містити connection/accept, serialized worker wait та transport overhead; без окремого timestamp у черзі це не можна чесно назвати чистим queue wait.

Для послідовних запитів baseline `non_service_ns` дає transport/connection overhead. Для конкурентних клієнтів збільшення цієї різниці є evidence serial queueing разом із transport overhead.

Timing не змінює protocol v1, opcode semantics, worker serialization, scheduling policy чи semantic result. Це лише diagnostic evidence.
