#!/usr/bin/env python3
"""Keep CML from re-minting SENS domain widths at compiler boundaries."""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TARGETS = (
    ROOT / "src/sens_compiler_export.rs",
)

DOMAIN_WIDTH_GUESS = re.compile(
    r'"D\d+"\s+if\s+bits\.len\(\)\s*==\s*\d+'
)


def main() -> int:
    violations: list[str] = []
    for path in TARGETS:
        text = path.read_text(encoding="utf-8")
        for match in DOMAIN_WIDTH_GUESS.finditer(text):
            violations.append(f"{path.relative_to(ROOT)}: {match.group(0)}")

    # Self-test the ratchet without importing project code.
    assert DOMAIN_WIDTH_GUESS.search('"D3" if bits.len() == 3')
    assert not DOMAIN_WIDTH_GUESS.search(
        '("D3", identity @ sens::DomainIdentity::D3(_)) => Ok(identity)'
    )

    if violations:
        print("CML-SENS-WIDTH-AUTHORITY: FAIL")
        print("\n".join(violations))
        print("CML must ask SENS to materialize exact identity; it must not guess Dn width.")
        return 1

    print("CML-SENS-WIDTH-AUTHORITY: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
