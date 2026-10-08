#!/usr/bin/env python3
"""Тести механічного L3 CUDA адаптера конформансу (#512)."""

from __future__ import annotations

import copy
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent
SENS_DIR = REPO_ROOT / "external" / "sens"

sys.path.insert(0, str(HERE))
import cml_cuda_l3_conformance as adapter


def make_valid_l0_row() -> dict:
    """Створює валідний еталонний L0 ORACLE рядок згідно зі схемою sens-execution-conformance/v1."""
    program_encoding = "canonical-source"
    program = "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(10 20 30))"
    program_digest = adapter.sha256_text(program)
    case_identity = {
        "contract": "11.8",
        "program_encoding": program_encoding,
        "program": program,
    }
    case_id = "case-" + adapter.sha256_text(adapter.canonical_json(case_identity))

    trace = [
        {"domain": 1, "bits": "1"},
        {"domain": 3, "bits": "001"},
    ]
    trace_digest = adapter.structured_digest(trace)

    observable = {
        "result_kind": "VALUE",
        "value": "#i32(11 21 31)",
        "output": "",
        "error_kind": None,
        "order_trace": [],
        "mechanism_status": "CALLABLE",
    }
    obs_digest = adapter.structured_digest(observable)

    return {
        "schema": "sens-execution-conformance/v1",
        "case_id": case_id,
        "contract": "11.8",
        "upstream_sha": "a" * 40,
        "producer_layer": "L0",
        "producer": "sens-eval-l0",
        "program_encoding": program_encoding,
        "program": program,
        "program_digest": program_digest,
        "identity_trace": trace,
        "identity_trace_digest": trace_digest,
        "observable": observable,
        "observable_digest": obs_digest,
        "oracle_digest": obs_digest,
        "parity_status": "ORACLE",
        "evidence_scope": "fixture",
        "exhaustive_bound": None,
        "legacy_identity_used": False,
    }


def test_matching_observation_yields_l3_pass():
    l0 = make_valid_l0_row()
    actual = copy.deepcopy(l0["observable"])

    l3 = adapter.build_cuda_l3_row(l0, actual)
    assert l3["producer_layer"] == "L3"
    assert l3["producer"] == adapter.DEFAULT_PRODUCER
    assert l3["parity_status"] == "PASS"
    assert l3["observable_digest"] == l0["oracle_digest"]
    assert l3["legacy_identity_used"] is False

    # Перевірка upstream-валідатором
    adapter.validate_via_upstream(l3, SENS_DIR)
    print("[PASS] test_matching_observation_yields_l3_pass")


def test_divergent_observation_yields_l3_fail_and_validates():
    l0 = make_valid_l0_row()
    # Мутуємо спостереження: результат розбіжний
    actual = {
        "result_kind": "VALUE",
        "value": "#i32(99 99 99)",
        "output": "",
        "error_kind": None,
        "order_trace": [],
        "mechanism_status": "CALLABLE",
    }

    l3 = adapter.build_cuda_l3_row(l0, actual)
    assert l3["producer_layer"] == "L3"
    assert l3["parity_status"] == "FAIL"
    assert l3["observable_digest"] != l0["oracle_digest"]

    # Навіть рядок зі статусом FAIL є валідним для схеми конформансу!
    adapter.validate_via_upstream(l3, SENS_DIR)
    print("[PASS] test_divergent_observation_yields_l3_fail_and_validates")


def test_stale_contract_fails_closed():
    l0 = make_valid_l0_row()
    l0["contract"] = "11.5"

    actual = copy.deepcopy(l0["observable"])
    try:
        adapter.build_cuda_l3_row(l0, actual)
    except ValueError as exc:
        assert "contract" in str(exc)
    else:
        raise AssertionError("Stale contract must fail closed")
    print("[PASS] test_stale_contract_fails_closed")


def test_legacy_identity_fails_closed():
    l0 = make_valid_l0_row()
    l0["legacy_identity_used"] = True

    actual = copy.deepcopy(l0["observable"])
    try:
        adapter.build_cuda_l3_row(l0, actual)
    except ValueError as exc:
        assert "legacy identity" in str(exc)
    else:
        raise AssertionError("Legacy identity must fail closed")
    print("[PASS] test_legacy_identity_fails_closed")


def test_malformed_observable_fails_closed():
    l0 = make_valid_l0_row()
    # Missing required keys
    actual = {"result_kind": "VALUE", "value": "123"}
    try:
        adapter.build_cuda_l3_row(l0, actual)
    except ValueError as exc:
        assert "missing keys" in str(exc)
    else:
        raise AssertionError("Malformed observable must fail closed")
    print("[PASS] test_malformed_observable_fails_closed")


def test_cuda_provenance_sidecar_generation():
    l0 = make_valid_l0_row()
    case_id = l0["case_id"]
    producer = "cml-cuda-nvrtc-driver-jit"

    provenance = {
        "cml_ir_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "ptx_digest": "fnv1a64:abcdef0123456789",
        "compute_capability": "6.1",
        "device_ordinal": 0,
        "device_name": "NVIDIA GeForce GTX 1050 Ti",
        "nvrtc_version": "12.6",
        "driver_version": 13000,
        "kernel_mode": "Production",
        "compile_options": ["-arch=compute_61", "-O3"],
        "launch_geometry": {
            "grid_dim": [977, 1, 1],
            "block_dim": [1024, 1, 1],
            "shared_mem_bytes": 0,
        },
        "latency_breakdown": {
            "cml_ir_lowering_ns": 211047,
            "nvrtc_compile_ns": 42381600,
            "driver_jit_load_ns": 341900,
            "function_lookup_ns": 92500,
            "htod_transfer_ns": 978700,
            "kernel_execution_ns": 265800,
            "dtoh_transfer_ns": 1464700,
            "total_cold_ns": 256570000,
            "total_warm_ns": 138276,
            "cpu_reference_ns": 325860000,
        },
    }

    sidecar = adapter.build_cuda_provenance_sidecar(case_id, producer, provenance)
    assert sidecar["schema"] == "cml-cuda-provenance-sidecar/v1"
    assert sidecar["case_id"] == case_id
    assert sidecar["producer"] == producer
    assert sidecar["compute_capability"] == "6.1"
    assert sidecar["driver_version"] == 13000
    assert "sidecar_digest" in sidecar
    assert len(sidecar["sidecar_digest"]) == 64
    print("[PASS] test_cuda_provenance_sidecar_generation")


def main() -> int:
    test_matching_observation_yields_l3_pass()
    test_divergent_observation_yields_l3_fail_and_validates()
    test_stale_contract_fails_closed()
    test_legacy_identity_fails_closed()
    test_malformed_observable_fails_closed()
    test_cuda_provenance_sidecar_generation()
    print("\nALL CML CUDA L3 CONFORMANCE TESTS PASSED.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
