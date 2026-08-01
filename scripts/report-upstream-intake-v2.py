#!/usr/bin/env python3
"""Print an honest, local-only coverage dashboard for the v2 source ledger.

This tool deliberately reports ledger coverage, not product capability.  A
component marked ``adapt`` is only source/rights evidence; it is not runnable
until it separately passes SessionActor, rebuilt-binary, CI, and release gates.
"""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LOCK = ROOT / "third_party/upstream-lock.v2.json"


def load_lock(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError("upstream lock root must be an object")
    return value


def dashboard(lock: dict[str, Any]) -> dict[str, Any]:
    expected = lock.get("expected_source_ids", [])
    sources = lock.get("sources", [])
    if not isinstance(expected, list) or not isinstance(sources, list):
        raise ValueError("lock expected_source_ids and sources must be lists")
    components = [component for source in sources for component in source.get("components", [])]
    dispositions = Counter(component.get("disposition", "missing") for component in components)
    asset_kinds = Counter(component.get("asset_kind", "missing") for component in components)
    source_gates = Counter(source.get("source_gate_status", "missing") for source in sources)
    source_rights = Counter(source.get("rights_status", "missing") for source in sources)
    exact_one = sum(1 for component in components if isinstance(component.get("disposition"), str))
    runnable = sum(1 for component in components if component.get("execution_authority") != "none")
    return {
        "schema_version": 1,
        "lock_status": lock.get("status"),
        "scope": "v2 selected component ledger; not a tree-wide source inventory",
        "source_records": {"expected": len(expected), "present": len(sources), "source_gate_status": dict(sorted(source_gates.items())), "rights_status": dict(sorted(source_rights.items()))},
        "components": {
            "inventory_total": len(components),
            "exact_one_disposition": exact_one,
            "dispositions": dict(sorted(dispositions.items())),
            "asset_kinds": dict(sorted(asset_kinds.items())),
            "runnable_from_intake": runnable,
        },
        "evidence_level": {
            "admitted_E2": 0,
            "actor_E3": 0,
            "product_E4": 0,
            "CI_E5": 0,
            "reason": "This ledger has no authority to claim execution, product, CI, or release proof.",
        },
        "blocked": lock.get("blocked_by", []),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    parser.add_argument("--check", action="store_true", help="fail if a draft dashboard would overclaim runnable intake")
    args = parser.parse_args()
    try:
        report = dashboard(load_lock(args.lock))
        if args.check and report["lock_status"] == "draft" and report["components"]["runnable_from_intake"] != 0:
            raise ValueError("a draft intake ledger cannot contain a runnable component")
        print(json.dumps(report, indent=2, sort_keys=True))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
