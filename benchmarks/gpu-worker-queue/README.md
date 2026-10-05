# Вимірювання черги спільного CUDA worker

Цей slice для #476 додає лише опційне timing evidence до вже існуючого
`cml-gpu-worker` протоколу v1.

Увімкнення:

```sh
CML_GPU_WORKER_TIMING=1 cml-gpu-worker serve
```

Клієнт зберігає старий формат відповіді без timing, якщо змінна не задана:

```sh
CML_GPU_WORKER_TIMING=1 cml-gpu-worker chain-file-i32 input.bin output.bin 1 2 3
```

Під timing клієнт друкує:

```text
client_total_ns=... count=... steps=... service_ns=... cuda_ns=... output=...
```

Визначення:

- `service_ns` — монотонний час server-side від початку обробки request до моменту перед відправленням response; сюди входить decode, file I/O та CUDA execution, але не фінальний запис response bytes.
- `cuda_ns` — вже наявний вимір навколо CUDA chain execution.
- `client_total_ns` — повний час одного клієнтського round-trip.
- `non_service_ns = client_total_ns - service_ns` — явна різниця, що містить connection/accept/protocol та інший час поза server service. Вона не називається «чистим queue wait», бо без окремого instrumentation queue і protocol не можна чесно розділити.

Для послідовних запитів `non_service_ns` дає baseline transport/connection накладних витрат. Для конкурентних клієнтів приріст цієї величини є evidence черги плюс transport overhead.

Протокол v1, opcode 4 та semantic result не змінюються. Timing є виключно diagnostic evidence і не впливає на результат обчислення.