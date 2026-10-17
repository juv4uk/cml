#!/usr/bin/env python3
"""Bind validated SENS oracle rows to one explicit CML compiler-pass boundary.

This is evidence plumbing only. It imports the already-merged conformance
adapter instead of duplicating the upstream schema/validator.

Current exact-domain rows are deliberately BLOCKED at ast-to-ir.lower while
CML's frontend/AST still lacks variable-width DomainIdentity transport.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import subprocess
import sys
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


def pass_manifest(binary: Path) -> dict[str, int]:
    proc = subprocess.run(
        [str(binary), "passes"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    passes: dict[str, int] = {}
    for raw in proc.stdout.splitlines():
        if not raw.startswith("CML-PASS\t"):
            continue
        fields = {}
        for part in raw.split("\t")[1:]:
            if "=" in part:
                key, value = part.split("=", 1)
                fields[key] = value
        pass_id = fields.get("id")
        version = fields.get("version")
        if not pass_id or not version:
            raise ValueError(f"malformed CML pass manifest line: {raw!r}")
        passes[pass_id] = int(version)
    if not passes:
        raise ValueError("CML pass manifest is empty")
    return passes


def exact_domain_transport_blocker(row: dict[str, object]) -> str | None:
    if row.get("program_encoding") != "canonical-source":
        return "unsupported-program-encoding"

    trace = row.get("identity_trace")
    if not isinstance(trace, list):
        raise ValueError("validated row unexpectedly lacks identity_trace array")

    # Current CML AST/IR function identity is still Sid8. A validated D1-D7
    # exact-domain trace therefore cannot be routed through ast-to-ir.lower
    # without changing identity. Fail closed rather than zero-padding/naming.
    if any(
        isinstance(item, dict)
        and isinstance(item.get("domain"), int)
        and item["domain"] != 8
        for item in trace
    ):
        return "exact-domain-identity-transport-unavailable"
    return "pass-execution-evidence-not-wired"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("jsonl", type=Path)
    ap.add_argument("--sens-dir", type=Path, required=True)
    ap.add_argument("--cml-compile", type=Path, required=True)
    ap.add_argument("--pass-id", required=True)
    args = ap.parse_args()

    adapter = load_adapter()
    validator = adapter.upstream_validator(args.sens_dir.resolve())
    adapter.validate_upstream(validator, args.jsonl.resolve())
    rows = adapter.read_oracle_rows(args.jsonl.resolve())

    passes = pass_manifest(args.cml_compile.resolve())
    if args.pass_id not in passes:
        raise ValueError(f"unknown CML pass id: {args.pass_id}")

    version = passes[args.pass_id]
    emitted = 0
    for row in sorted(rows, key=lambda item: str(item["case_id"])):
        reason = exact_domain_transport_blocker(row)
        status = "BLOCKED" if reason else "PASS"
        output_digest = "UNAVAILABLE" if status == "BLOCKED" else str(row["oracle_digest"])
        print(
            "CML-PASS-EVIDENCE"
            f"\tpass_id={args.pass_id}"
            f"\tversion={version}"
            f"\tcontract={row['contract']}"
            f"\tupstream_sha={row['upstream_sha']}"
            f"\tcase_id={row['case_id']}"
            f"\toracle_digest={row['oracle_digest']}"
            f"\tidentity_trace_digest={row['identity_trace_digest']}"
            f"\tpass_output_digest={output_digest}"
            f"\tstatus={status}"
            f"\treason={reason or 'none'}"
        )
        emitted += 1

    print(f"CML-PASS-EVIDENCE-GREEN pass_id={args.pass_id} rows={emitted}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (
        OSError,
        ValueError,
        RuntimeError,
        json.JSONDecodeError,
        subprocess.CalledProcessError,
    ) as exc:
        print(f"CML-PASS-EVIDENCE-RED: {exc}", file=sys.stderr)
        raise SystemExit(2)
