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
            "requires juv4uk/sens#3575 or successor"
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("jsonl", type=Path)
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

    for row in rows:
        observable = row["observable"]
        print(
            "CML-CONFORMANCE-CASE "
            f"case_id={row['case_id']} "
            f"contract={row['contract']} "
            f"oracle_digest={row['oracle_digest']} "
            f"mechanism={observable['mechanism_status']}"
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
