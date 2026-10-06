#!/usr/bin/env python3
# Копія juv4uk/sens scripts/gpu-worker-parity.py (канонічна). Змінюй там, не тут.
"""Справжня GPU-перевірка CI: CPU-референс проти CUDA-воркера cml.

Прогоняє ланцюг `x -> x + k1 + k2 + ...` над i32-буфером через спільний
воркер (`cml-gpu-worker chain-file-i32-provenance`), порівнює побайтово з
референсом на CPU і додає негативний контроль (переповнення i32 має бути
відхилене, а не обернуте). Будь-яка відсутність CUDA, воркера чи розбіжність
результату -- ІМЕНОВАНА помилка (exit != 0), ніколи не тихий зелений.

Це перевірка механізму (воркер реально виконав ядро на GPU), а не семантики
SENS: паритет з оракулом SENS чекає закон з sens#3766 (оракул лишається
BLOCKED-MECHANISM, sens#3889). Замок допуску бере сам воркер на опкодах 4/5, тому
викликати цей скрипт під зовнішнім `flock` НЕ можна (самоблокування).

Використання:
    python3 scripts/gpu-worker-parity.py [--evidence evidence.json]
"""

import argparse
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile

N = 1 << 20
OFFSETS = [1, 2, 3]
CALL_TIMEOUT = 120
SOCKET_DEFAULT = "/run/cml-gpu-worker/worker.sock"


class Failure(Exception):
    """Іменована причина відмови (код виходу 1)."""


def worker_path():
    found = shutil.which("cml-gpu-worker")
    if found:
        return found
    local = os.path.expanduser("~/.local/bin/cml-gpu-worker")
    if os.access(local, os.X_OK):
        return local
    raise Failure("WORKER_BINARY_MISSING: cml-gpu-worker не знайдено ні в PATH, ні в ~/.local/bin")


def call(worker, *args):
    env = dict(os.environ)
    env.setdefault("CML_GPU_WORKER_SOCKET", SOCKET_DEFAULT)
    try:
        done = subprocess.run(
            [worker, *args], capture_output=True, text=True, timeout=CALL_TIMEOUT, env=env
        )
    except subprocess.TimeoutExpired:
        raise Failure(f"WORKER_TIMEOUT: {args[0]} не відповів за {CALL_TIMEOUT} с")
    return done.returncode, done.stdout.strip(), done.stderr.strip()


def field(text, name):
    match = re.search(rf"(?:^|\s){name}=(\S+)", text)
    return match.group(1) if match else None


def ascii_field(value, fallback):
    cleaned = re.sub(r"[^A-Za-z0-9._/-]", "-", value or fallback)[:120]
    return cleaned or fallback


def reference_input():
    # Детермінований LCG; значення обмежені, щоб сума не виходила за i32.
    state = 0x2545F491
    values = []
    for _ in range(N):
        state = (state * 1103515245 + 12345) & 0x7FFFFFFF
        values.append(state % 2_000_001 - 1_000_000)
    return values


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", help="куди записати JSON-свідчення")
    options = parser.parse_args()
    evidence = {"schema": "gpu-worker-parity/1", "offsets": OFFSETS, "elements": N}

    worker = worker_path()
    code, out, err = call(worker, "ping")
    if code != 0 or out != "pong":
        raise Failure(f"WORKER_UNREACHABLE: ping -> code={code} out={out!r} err={err!r}")

    code, out, err = call(worker, "probe")
    if code != 0 or "Nvidia" not in out or "Cuda" not in out:
        raise Failure(f"CUDA_DEVICE_ABSENT: probe -> code={code} out={out!r} err={err!r}")
    name = re.search(r'name: "([^"]+)"', out)
    capability = re.search(r"compute_capability: \((\d+), (\d+)\)", out)
    memory = re.search(r"total_memory_bytes: (\d+)", out)
    if not (name and capability and memory):
        raise Failure(f"CUDA_DEVICE_ABSENT: probe не описує пристрій повністю: {out!r}")
    evidence["device"] = {
        "name": name.group(1),
        "compute_capability": f"{capability.group(1)}.{capability.group(2)}",
        "total_memory_bytes": int(memory.group(1)),
    }

    code, out, err = call(worker, "add-i32", "7", "1", "2", "3", "4")
    if code != 0 or out != "8 9 10 11":
        raise Failure(f"CUDA_KERNEL_WRONG: add-i32 -> code={code} out={out!r} err={err!r}")

    values = reference_input()
    expected = [value + sum(OFFSETS) for value in values]
    with tempfile.TemporaryDirectory(prefix="gpu-worker-parity-") as tmp:
        source = os.path.join(tmp, "in.bin")
        result = os.path.join(tmp, "out.bin")
        with open(source, "wb") as handle:
            handle.write(struct.pack(f"<{N}i", *values))

        repository = ascii_field(os.environ.get("GITHUB_REPOSITORY"), "local/sens")
        run_id = ascii_field(os.environ.get("GITHUB_RUN_ID"), "local-run")
        job = ascii_field(os.environ.get("GITHUB_JOB"), "local-job")
        code, out, err = call(
            worker, "chain-file-i32-provenance", repository, run_id, job,
            "add-chain-i32-1m", source, result, *map(str, OFFSETS),
        )
        if code != 0:
            raise Failure(f"CUDA_CHAIN_FAILED: code={code} err={err!r} out={out!r}")
        for name in ("admission_wait_ns", "cuda_ns"):
            if field(out, name) is None:
                raise Failure(f"EVIDENCE_FIELD_MISSING: у відповіді воркера немає {name}: {out!r}")
        if int(field(out, "cuda_ns")) <= 0:
            raise Failure(f"GPU_NOT_USED: cuda_ns={field(out, 'cuda_ns')} (ядро не виконувалось)")
        if not os.path.exists(result) or os.path.getsize(result) != N * 4:
            raise Failure("CUDA_OUTPUT_MISSING: файл результату відсутній або має хибний розмір")
        with open(result, "rb") as handle:
            actual = list(struct.unpack(f"<{N}i", handle.read()))
        if actual != expected:
            bad = next(i for i in range(N) if actual[i] != expected[i])
            raise Failure(
                f"PARITY_MISMATCH: індекс {bad}: GPU={actual[bad]} CPU-референс={expected[bad]}"
            )
        evidence["chain"] = {
            "admission_wait_ns": int(field(out, "admission_wait_ns")),
            "cuda_ns": int(field(out, "cuda_ns")),
            "parity": "GPU == CPU-референс для всіх елементів",
        }

        # Негативний контроль: переповнення i32 має відхилятись, а не обертатись.
        edge = os.path.join(tmp, "edge.bin")
        edge_out = os.path.join(tmp, "edge-out.bin")
        with open(edge, "wb") as handle:
            handle.write(struct.pack("<2i", 2147483000, 5))
        code, out, err = call(
            worker, "chain-file-i32-provenance", repository, run_id, job,
            "overflow-must-reject", edge, edge_out, "1000",
        )
        if code == 0 or os.path.exists(edge_out):
            raise Failure(
                f"OVERFLOW_NOT_REJECTED: воркер прийняв переповнення (code={code}, out={out!r})"
            )
        if "UnsupportedInput" not in err:
            raise Failure(f"OVERFLOW_WRONG_ERROR: очікувалось UnsupportedInput, отримано {err!r}")
        evidence["negative_control"] = "переповнення i32 відхилено: UnsupportedInput"

    print("GPU_WORKER_PARITY_GREEN " + json.dumps(evidence, ensure_ascii=False))
    if options.evidence:
        with open(options.evidence, "w", encoding="utf-8") as handle:
            json.dump(evidence, handle, ensure_ascii=False, indent=2)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Failure as failure:
        print(f"GPU_WORKER_PARITY_RED {failure}", file=sys.stderr)
        sys.exit(1)
