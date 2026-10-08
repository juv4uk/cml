#!/usr/bin/env python3
"""Механічний адаптер CML для побудови L3 CUDA рядків відповідності (#512).

Цей модуль реалізує закон поєднання валідованого рядка SENS L0 ORACLE
та фактичного спостереження CUDA у валідний рядок конформансу L3 (рівень GPU).
CML не містить локальних таблиць очікуваних результатів і не винаходить істину оракула:
- Усі семантичні поля, програма, identity_trace та oracle_digest копіюються з L0;
- observable формується виключно з фактичного спостереження виконання на CUDA;
- parity_status = PASS тоді і тільки тоді, коли observable_digest == oracle_digest;
- Механічний provenance (NVRTC, Driver JIT, PTX, затримки) записується в окремий sidecar;
- Кожен сформований L3 рядок верифікується upstream-валідатором sens-execution-conformance/v1.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

SCHEMA = "sens-execution-conformance/v1"
SIDECAR_SCHEMA = "cml-cuda-provenance-sidecar/v1"
CURRENT_CONTRACT = "11.8"
PRODUCER_LAYER = "L3"
DEFAULT_PRODUCER = "cml-cuda-nvrtc-driver-jit"

REQUIRED_OBSERVABLE_KEYS = {
    "result_kind",
    "value",
    "output",
    "error_kind",
    "order_trace",
    "mechanism_status",
}


def canonical_json(value: Any) -> str:
    """Канонічний детерміністичний JSON без пробілів з відсортованими ключами."""
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    )


def sha256_text(value: str) -> str:
    """Обчислення SHA-256 від UTF-8 рядка."""
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def structured_digest(value: Any) -> str:
    """Канонічний дайджест структури через канонічний JSON."""
    return sha256_text(canonical_json(value))


def validate_l0_row(row: dict[str, Any]) -> None:
    """Fail-closed перевірка того, що рядок є дійсним L0 ORACLE-рядком."""
    if not isinstance(row, dict):
        raise ValueError("L0 row must be an object")
    if row.get("schema") != SCHEMA:
        raise ValueError(f"L0 row schema must be {SCHEMA!r}, got {row.get('schema')!r}")
    if row.get("contract") != CURRENT_CONTRACT:
        raise ValueError(
            f"L0 row contract must be {CURRENT_CONTRACT!r}, got {row.get('contract')!r}"
        )
    if row.get("producer_layer") != "L0":
        raise ValueError(
            f"L0 row producer_layer must be 'L0', got {row.get('producer_layer')!r}"
        )
    if row.get("parity_status") != "ORACLE":
        raise ValueError(
            f"L0 row parity_status must be 'ORACLE', got {row.get('parity_status')!r}"
        )
    if row.get("legacy_identity_used") is not False:
        raise ValueError("legacy identity is forbidden in SENS execution conformance")

    oracle_digest = row.get("oracle_digest")
    observable_digest = row.get("observable_digest")
    if not oracle_digest or oracle_digest != observable_digest:
        raise ValueError("L0 oracle row must satisfy oracle_digest == observable_digest")


def validate_observable(observable: dict[str, Any]) -> None:
    """Перевірка форми та типів фактичного спостереження."""
    if not isinstance(observable, dict):
        raise ValueError("actual observable must be a dict")
    missing = sorted(REQUIRED_OBSERVABLE_KEYS - observable.keys())
    extra = sorted(observable.keys() - REQUIRED_OBSERVABLE_KEYS)
    if missing:
        raise ValueError(f"observable missing keys: {missing}")
    if extra:
        raise ValueError(f"observable has unexpected keys: {extra}")

    kind = observable["result_kind"]
    if kind not in {"VALUE", "ERROR", "BLOCKED-MECHANISM", "RESEARCH-DOMAIN"}:
        raise ValueError(f"invalid observable result_kind: {kind!r}")

    mechanism = observable["mechanism_status"]
    if mechanism not in {"CALLABLE", "BLOCKED-MECHANISM", "RESEARCH-DOMAIN", "INVALID"}:
        raise ValueError(f"invalid observable mechanism_status: {mechanism!r}")

    value = observable["value"]
    if value is not None and not isinstance(value, str):
        raise ValueError("observable value must be string or null")

    output = observable["output"]
    if not isinstance(output, str):
        raise ValueError("observable output must be a string")

    error_kind = observable["error_kind"]
    if error_kind is not None and (not isinstance(error_kind, str) or not error_kind):
        raise ValueError("observable error_kind must be a non-empty string or null")

    order_trace = observable["order_trace"]
    if not isinstance(order_trace, list) or any(not isinstance(x, str) for x in order_trace):
        raise ValueError("observable order_trace must be a list of strings")


def build_cuda_l3_row(
    l0_row: dict[str, Any],
    actual_observable: dict[str, Any],
    producer: str = DEFAULT_PRODUCER,
) -> dict[str, Any]:
    """Будує валідний L3 рядок конформансу з L0 рядка та фактичного CUDA спостереження."""
    validate_l0_row(l0_row)
    validate_observable(actual_observable)

    actual_digest = structured_digest(actual_observable)
    oracle_digest = l0_row["oracle_digest"]
    parity_status = "PASS" if actual_digest == oracle_digest else "FAIL"

    l3_row = {
        "schema": SCHEMA,
        "case_id": l0_row["case_id"],
        "contract": l0_row["contract"],
        "upstream_sha": l0_row["upstream_sha"],
        "producer_layer": PRODUCER_LAYER,
        "producer": producer,
        "program_encoding": l0_row["program_encoding"],
        "program": l0_row["program"],
        "program_digest": l0_row["program_digest"],
        "identity_trace": l0_row["identity_trace"],
        "identity_trace_digest": l0_row["identity_trace_digest"],
        "observable": actual_observable,
        "observable_digest": actual_digest,
        "oracle_digest": oracle_digest,
        "parity_status": parity_status,
        "evidence_scope": l0_row["evidence_scope"],
        "exhaustive_bound": l0_row["exhaustive_bound"],
        "legacy_identity_used": False,
    }
    return l3_row


def build_cuda_provenance_sidecar(
    case_id: str,
    producer: str,
    provenance: dict[str, Any],
) -> dict[str, Any]:
    """Будує окремий sidecar механічного provenance для збереження деталей виконання CUDA."""
    if not isinstance(case_id, str) or not case_id.startswith("case-"):
        raise ValueError(f"invalid case_id: {case_id!r}")
    if not isinstance(producer, str) or not producer:
        raise ValueError("producer must be a non-empty string")
    if not isinstance(provenance, dict):
        raise ValueError("provenance must be a dict")

    sidecar = {
        "schema": SIDECAR_SCHEMA,
        "case_id": case_id,
        "producer": producer,
        "cml_ir_digest": provenance.get("cml_ir_digest"),
        "ptx_digest": provenance.get("ptx_digest"),
        "compute_capability": provenance.get("compute_capability"),
        "device_ordinal": provenance.get("device_ordinal"),
        "device_name": provenance.get("device_name"),
        "nvrtc_version": provenance.get("nvrtc_version"),
        "driver_version": provenance.get("driver_version"),
        "kernel_mode": provenance.get("kernel_mode"),
        "compile_options": provenance.get("compile_options", []),
        "launch_geometry": provenance.get("launch_geometry"),
        "latency_breakdown": provenance.get("latency_breakdown"),
    }
    sidecar["sidecar_digest"] = structured_digest(
        {k: v for k, v in sidecar.items() if k != "sidecar_digest"}
    )
    return sidecar


def validate_via_upstream(row: dict[str, Any], sens_dir: Path) -> None:
    """Запускає upstream валідатор sens-execution-conformance для гарантії відповідності."""
    validator = (
        sens_dir
        / "benchmarks"
        / "execution-ladder-conformance"
        / "validate.py"
    )
    if not validator.is_file():
        raise FileNotFoundError(f"upstream validator not found at: {validator}")

    row_json_str = canonical_json(row)
    cmd = [
        sys.executable,
        "-c",
        (
            f"import sys, json; sys.path.insert(0, {str(validator.parent)!r}); "
            f"from validate import validate; validate(json.loads({row_json_str!r}))"
        ),
    ]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        raise ValueError(
            f"Upstream validation failed for L3 row:\nSTDOUT: {res.stdout}\nSTDERR: {res.stderr}"
        )
