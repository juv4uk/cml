#!/usr/bin/env python3
"""Fail-closed адаптер CML до канонічного SENS execution-conformance JSONL.

Цей файл не дублює схему і не містить локальної таблиці очікуваних значень.
Спочатку він запускає upstream-валідатор із external/sens, а вже потім
перевіряє лише те, що CML справді споживає як L2-вхід.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

SCHEMA = "sens-execution-conformance/v1"
CURRENT_CONTRACT = "11.6"


def upstream_validator(sens_dir: Path) -> Path:
    path = (
        sens_dir
        / "benchmarks"
        / "execution-ladder-conformance"
        / "validate.py"
    )
    if not path.is_file():
        raise FileNotFoundError(
            f"upstream SENS conformance validator not found: {path}; "
            "requires juv4uk/sens#3593/#3594 or current successor"
        )
    return path


def validate_upstream(validator: Path, jsonl: Path) -> None:
    subprocess.run(
        [sys.executable, str(validator), str(jsonl)],
        check=True,
    )


def read_oracle_rows(jsonl: Path) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    for line_no, raw in enumerate(
        jsonl.read_text(encoding="utf-8").splitlines(), 1
    ):
        if not raw.strip():
            continue
        row = json.loads(raw)
        if row.get("schema") != SCHEMA:
            raise ValueError(f"line {line_no}: unexpected schema")
        if row.get("contract") != CURRENT_CONTRACT:
            raise ValueError(
                f"line {line_no}: current CML import requires "
                f"contract={CURRENT_CONTRACT}, got {row.get('contract')!r}"
            )
        if row.get("producer_layer") != "L0":
            raise ValueError(
                f"line {line_no}: CML import expects upstream L0 oracle rows"
            )
        if row.get("parity_status") != "ORACLE":
            raise ValueError(
                f"line {line_no}: L0 row must have parity_status=ORACLE"
            )
        if row.get("legacy_identity_used") is not False:
            raise ValueError(
                f"line {line_no}: legacy identity is forbidden"
            )
        rows.append(row)
    if not rows:
        raise ValueError("oracle JSONL is empty")
    return rows


def lisp_string(value: str) -> str:
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'


def render_fpga_handoff(row: dict[str, object]) -> str:
    observable = row["observable"]
    trace = row["identity_trace"]
    if not isinstance(observable, dict) or not isinstance(trace, list):
        raise ValueError("validated row has invalid observable/identity_trace shape")

    trace_rows: list[str] = []
    for item in trace:
        if not isinstance(item, dict):
            raise ValueError("identity_trace item must be an object")
        domain = item.get("domain")
        bits = item.get("bits")
        if not isinstance(domain, int) or not isinstance(bits, str):
            raise ValueError("identity_trace item missing domain/bits")
        trace_rows.append(f"((domain . {domain}) (bits . {lisp_string(bits)}))")

    scope = row.get("evidence_scope")
    bound = row.get("exhaustive_bound")
    bound_form = "()"
    if scope == "bounded-exhaustive":
        if not isinstance(bound, dict):
            raise ValueError("bounded-exhaustive row missing exhaustive_bound")
        grammar = bound.get("grammar_profile")
        domain_set = bound.get("domain_set")
        if not isinstance(grammar, str) or not grammar:
            raise ValueError("bounded-exhaustive row missing grammar_profile")
        if not isinstance(domain_set, list) or not all(isinstance(x, int) for x in domain_set):
            raise ValueError("bounded-exhaustive row missing domain_set")
        bound_form = (
            "((grammar-profile . " + lisp_string(grammar) + ") "
            "(domain-set . (" + " ".join(str(x) for x in domain_set) + ")) "
            f"(max-ast-depth . {bound.get('max_ast_depth')}) "
            f"(max-nodes . {bound.get('max_nodes')}) "
            f"(argument-value-bound . {bound.get('argument_value_bound')}))"
        )

    required_strings = [
        "case_id", "contract", "upstream_sha", "oracle_digest",
        "identity_trace_digest", "program_digest",
    ]
    for key in required_strings:
        if not isinstance(row.get(key), str):
            raise ValueError(f"validated row missing {key}")

    mechanism = observable.get("mechanism_status")
    if not isinstance(mechanism, str):
        raise ValueError("validated row missing observable.mechanism_status")

    return (
        "(fpga-conformance-handoff/v1 "
        f"(case-id . {lisp_string(row['case_id'])}) "
        f"(contract . {lisp_string(row['contract'])}) "
        f"(upstream-sha . {lisp_string(row['upstream_sha'])}) "
        f"(program-digest . {lisp_string(row['program_digest'])}) "
        f"(oracle-digest . {lisp_string(row['oracle_digest'])}) "
        f"(identity-trace-digest . {lisp_string(row['identity_trace_digest'])}) "
        f"(mechanism-status . {lisp_string(mechanism)}) "
        f"(evidence-scope . {lisp_string(str(scope or 'case'))}) "
        f"(exhaustive-bound . {bound_form}) "
        "(identity-trace . (" + " ".join(trace_rows) + "))"
        ")"
    )


def write_fpga_handoff(rows: list[dict[str, object]], path: Path) -> None:
    payload = "\n".join(render_fpga_handoff(row) for row in rows) + "\n"
    path.write_text(payload, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("jsonl", type=Path)
    parser.add_argument(
        "--fpga-handoff-out",
        type=Path,
        help="write validated transport-only FPGA handoff rows as Lisp data",
    )
    parser.add_argument(
        "--sens-dir",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "external" / "sens",
    )
    args = parser.parse_args()

    jsonl = args.jsonl.resolve()
    sens_dir = args.sens_dir.resolve()
    validator = upstream_validator(sens_dir)

    validate_upstream(validator, jsonl)
    rows = read_oracle_rows(jsonl)

    if args.fpga_handoff_out is not None:
        write_fpga_handoff(rows, args.fpga_handoff_out.resolve())

    for row in rows:
        observable = row["observable"]
        print(
            "CML-CONFORMANCE-CASE "
            f"case_id={row['case_id']} "
            f"contract={row['contract']} "
            f"oracle_digest={row['oracle_digest']} "
            f"mechanism={observable['mechanism_status']}"
        )
    if args.fpga_handoff_out is not None:
        print(
            f"CML-FPGA-HANDOFF-GREEN cases={len(rows)} "
            f"path={args.fpga_handoff_out.resolve()}"
        )
    print(f"CML-CONFORMANCE-IMPORT-GREEN cases={len(rows)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (
        OSError,
        ValueError,
        json.JSONDecodeError,
        subprocess.CalledProcessError,
    ) as exc:
        print(f"CML-CONFORMANCE-IMPORT-RED: {exc}", file=sys.stderr)
        raise SystemExit(2)
