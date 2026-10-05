#!/usr/bin/env python3
"""Focused tests for the non-semantic SENS conformance adapter (#449)."""

from __future__ import annotations

import importlib.util
import json
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ADAPTER = HERE / "check-sens-execution-conformance.py"


def load_adapter():
    spec = importlib.util.spec_from_file_location("sens_conformance_adapter", ADAPTER)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load adapter: {ADAPTER}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_row(path: Path, contract: str) -> None:
    row = {
        "schema": "sens-execution-conformance/v1",
        "contract": contract,
        "producer_layer": "L0",
        "parity_status": "ORACLE",
        "legacy_identity_used": False,
    }
    path.write_text(json.dumps(row) + "\n", encoding="utf-8")


def main() -> int:
    adapter = load_adapter()

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        current = root / "current.jsonl"
        old = root / "old.jsonl"

        write_row(current, "11.6")
        rows = adapter.read_oracle_rows(current)
        assert len(rows) == 1
        assert rows[0]["contract"] == "11.6"

        write_row(old, "11.5")
        try:
            adapter.read_oracle_rows(old)
        except ValueError as exc:
            assert "contract=11.6" in str(exc)
        else:
            raise AssertionError("Contract 11.5 row must fail closed")

    print("CML-CONFORMANCE-ADAPTER-SELFTEST: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
