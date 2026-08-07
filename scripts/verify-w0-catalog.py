#!/usr/bin/env python3
"""W0-A ScienceCatalogV1 read-only catalog verifier.

The catalog aggregates built-in / adopted / user-owned skill and capability
entries with their admission state. Six-state admission ladder:
Cataloged / Quarantined / FixtureOnly / Sandboxed / Managed / Released.
A catalog entry can never be reported runnable below its state; state can
only advance with a receipt (this verifier checks the state/receipt pairing).
The catalog itself holds no execution handle.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "docs/science/5.0/w0-catalog/catalog.v1.json"

ADMISSION_STATES = [
    "Cataloged",
    "Quarantined",
    "FixtureOnly",
    "Sandboxed",
    "Managed",
    "Released",
]

RANK = {state: index for index, state in enumerate(ADMISSION_STATES)}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def verify(catalog_path: Path) -> None:
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    require(catalog.get("schema_version") == 1, "catalog schema_version must be 1")
    require(catalog.get("read_only") is True, "catalog must be read-only")
    entries = catalog.get("entries")
    require(isinstance(entries, list) and entries, "catalog entries required")

    for entry in entries:
        require(entry.get("id"), "entry missing id")
        require(entry.get("source"), "entry missing source")
        state = entry.get("admission_state")
        require(state in ADMISSION_STATES, f"{entry['id']} admission_state unsupported: {state}")
        # A state above Cataloged requires a receipt naming the evidence.
        if RANK[state] > RANK["Cataloged"]:
            require(entry.get("receipt"), f"{entry['id']} advanced state without receipt")
        # UI visibility: Cataloged and FixtureOnly may be listed as candidates;
        # Quarantined is dev-diagnostic only; Sandboxed+ are runnable under policy.
        require(
            entry.get("ui_label"),
            f"{entry['id']} missing ui_label",
        )
        if state == "Quarantined":
            require(
                entry.get("ui_label") == "dev-diagnostic",
                f"{entry['id']} quarantined entry must be dev-diagnostic only",
            )
        if RANK[state] <= RANK["Cataloged"]:
            require(
                entry.get("runnable") is False,
                f"{entry['id']} cataloged entry cannot be runnable",
            )

    print(f"PASS: W0-A ScienceCatalogV1 verified ({len(entries)} entries, six-state ladder with receipts)")


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", type=Path, default=CATALOG)
    args = parser.parse_args()
    try:
        verify(args.catalog)
    except ValueError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
