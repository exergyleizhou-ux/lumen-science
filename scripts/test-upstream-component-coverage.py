#!/usr/bin/env python3
"""Focused proof that component coverage cannot conceal ambiguous paths."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/report-upstream-component-coverage.py"
LOCK = ROOT / "third_party/upstream-lock.v2.json"
SPEC = importlib.util.spec_from_file_location("component_coverage", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load component coverage script")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def main() -> int:
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    checked_in = MODULE.report(lock)
    conflicting = copy.deepcopy(lock)
    conflicting["sources"][0]["components"][1]["path"] = "biomni/tool/**"
    overlap = MODULE.report(conflicting)
    results = [
        (
            "checked-in report distinguishes selected coverage from the full nine-source tree inventory",
            checked_in["tree_entries"]["external_scanned"] == 12191
            and checked_in["tree_entries"]["unclassified_by_selected_components"] > 0,
        ),
        (
            "checked-in selected component paths have no ambiguous overlap",
            checked_in["tree_entries"]["matching_multiple_components"] == 0,
        ),
        (
            "overlapping selected paths are detected instead of silently double-dispositioned",
            overlap["tree_entries"]["matching_multiple_components"] > 0,
        ),
    ]
    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
