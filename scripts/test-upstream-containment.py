#!/usr/bin/env python3
"""Negative tests for the default-deny external source containment manifest."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/verify-upstream-containment.py"
MANIFEST = ROOT / "third_party/upstream-containment.v1.json"
LOCK = ROOT / "third_party/upstream-lock.v2.json"
SPEC = importlib.util.spec_from_file_location("containment", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load containment verifier")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def fails(manifest: dict[str, object], lock: dict[str, object]) -> bool:
    try:
        MODULE.verify(manifest, lock)
    except ValueError:
        return True
    return False


def main() -> int:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    unsafe_default = copy.deepcopy(manifest)
    unsafe_default["default"]["disposition"] = "adapt"
    missing_source = copy.deepcopy(manifest)
    missing_source["sources"].pop()
    overlap = copy.deepcopy(lock)
    overlap["sources"][0]["components"][1]["path"] = "biomni/tool/**"
    report = MODULE.verify(manifest, lock)
    results = [
        ("checked-in containment covers all external tree entries", report == {"external_tree_entries": 3139, "selected_component_entries": 481, "default_quarantined_entries": 2658}),
        ("unsafe default disposition fails", fails(unsafe_default, lock)),
        ("missing external source fails", fails(missing_source, lock)),
        ("overlapping component paths fail instead of double-dispositioning", fails(manifest, overlap)),
    ]
    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
