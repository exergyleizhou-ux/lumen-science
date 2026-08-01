#!/usr/bin/env python3
"""Prove every discovered external source entry has a fail-closed disposition.

Selected lock components are candidate classifications. Everything outside
those exact, non-overlapping paths is explicitly quarantined. The result is
containment only -- it is not an admission, rights, runtime, or product gate.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "third_party/upstream-lock.v2.json"
MANIFEST = ROOT / "third_party/upstream-containment.v1.json"
COMPONENT_REPORT = ROOT / "scripts/report-upstream-component-coverage.py"
SPEC = importlib.util.spec_from_file_location("component_coverage", COMPONENT_REPORT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load component coverage helpers")
COMPONENT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPONENT)


def require(value: bool, message: str) -> None:
    if not value:
        raise ValueError(message)


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"{path} root must be an object")
    return value


def verify(manifest: dict[str, Any], lock: dict[str, Any]) -> dict[str, int]:
    require(manifest.get("schema_version") == 1, "containment schema_version must be 1")
    require(manifest.get("status") == "active-containment", "containment must be active")
    require(hashlib.sha256((ROOT / "third_party/upstream-lock.v2.json").read_bytes()).hexdigest() == manifest.get("upstream_lock_sha256"), "containment lock digest is stale")
    default = manifest.get("default")
    require(default == {"disposition": "quarantine", "execution_authority": "none", "reuse_mode": "none"}, "containment default must be no-authority quarantine")
    expected = {source["id"] for source in lock["sources"] if not source["id"].startswith("exergyleizhou-ux-")}
    records = manifest.get("sources")
    require(isinstance(records, list), "containment sources must be a list")
    by_id = {record.get("source_id"): record for record in records if isinstance(record, dict)}
    require(set(by_id) == expected and len(by_id) == len(records), "containment must cover exactly the seven external sources")
    selected = 0
    defaulted = 0
    total = 0
    for source in lock["sources"]:
        source_id = source["id"]
        if source_id not in expected:
            continue
        entries = COMPONENT.source_inventory(source)
        require(entries is not None, f"{source_id} lacks a tree inventory")
        receipt = load(ROOT / source["components"][0]["evidence"]["record"])
        record = by_id[source_id]
        require(record.get("tree_sha256") == receipt["tree_inventory"]["sha256"], f"{source_id} tree digest mismatches receipt")
        require(record.get("entry_count") == len(entries), f"{source_id} entry count mismatches receipt")
        for entry in entries:
            require(entry.get("disposition") == "quarantine" and entry.get("execution_authority") == "none", f"{source_id}:{entry.get('path')} is not safely quarantined in its tree inventory")
            matches = [component for component in source["components"] if COMPONENT.tree_glob_matches(component["path"], entry["path"])]
            require(len(matches) <= 1, f"{source_id}:{entry['path']} matches multiple selected dispositions")
            if matches:
                selected += 1
            else:
                defaulted += 1
            total += 1
    require(total == 3139, f"unexpected external tree total: {total}")
    require(selected + defaulted == total, "some discovered source entries escaped containment")
    return {"external_tree_entries": total, "selected_component_entries": selected, "default_quarantined_entries": defaulted}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument("--lock", type=Path, default=LOCK)
    args = parser.parse_args()
    try:
        report = verify(load(args.manifest), load(args.lock))
        print("PASS: external tree containment " + json.dumps(report, sort_keys=True))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
