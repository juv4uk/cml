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
        "case_id": "case-" + "a" * 64,
        "upstream_sha": "b" * 40,
        "program_digest": "c" * 64,
        "oracle_digest": "d" * 64,
        "identity_trace_digest": "e" * 64,
        "identity_trace": [{"domain": 3, "bits": "001"}],
        "observable": {
            "mechanism_status": "CALLABLE",
            "result_kind": "VALUE",
            "value": "()",
            "error_kind": None,
            "output": "",
            "order_trace": [],
        },
        "evidence_scope": "bounded-exhaustive",
        "exhaustive_bound": {
            "grammar_profile": "d1-d3-structural-predicate-v1",
            "domain_set": [1, 2, 3],
            "max_ast_depth": 3,
            "max_nodes": 11,
            "argument_value_bound": 2,
        },
    }
    path.write_text(json.dumps(row) + "\n", encoding="utf-8")


def main() -> int:
    adapter = load_adapter()

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        current = root / "current.jsonl"
        old = root / "old.jsonl"

        write_row(current, "11.8")
        rows = adapter.read_oracle_rows(current)
        assert len(rows) == 1
        assert rows[0]["contract"] == "11.8"

        handoff = root / "fpga-handoff.lisp"
        adapter.write_fpga_handoff(rows, handoff)
        rendered = handoff.read_text(encoding="utf-8")
        assert rendered.startswith("(fpga-conformance-handoff/v1 ")
        assert '(contract . "11.8")' in rendered
        assert '(case-id . "case-' + "a" * 64 + '")' in rendered
        assert '(oracle-digest . "' + "d" * 64 + '")' in rendered
        assert '(identity-trace-digest . "' + "e" * 64 + '")' in rendered
        assert '(grammar-profile . "d1-d3-structural-predicate-v1")' in rendered
        assert '(identity-trace . (((domain . 3) (bits . "001"))))' in rendered

        write_row(old, "11.6")
        try:
            adapter.read_oracle_rows(old)
        except ValueError as exc:
            assert "contract=11.8" in str(exc)
        else:
            raise AssertionError("Contract 11.6 row must fail closed")

    print("CML-CONFORMANCE-ADAPTER-SELFTEST: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
