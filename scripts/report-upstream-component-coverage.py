#!/usr/bin/env python3
"""Report exactly how much of each external source tree has a disposition.

This is a discovery/triage report only.  It never turns a matched file into an
admitted or runnable capability: every tree entry remains quarantined until a
separate implementation and SessionActor product proof exists.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import Counter
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LOCK = ROOT / "third_party/upstream-lock.v2.json"


def tree_glob_matches(pattern: str, path: str) -> bool:
    """Match the lock's slash-aware relative ``*``/``**`` glob language."""
    expression = ""
    index = 0
    while index < len(pattern):
        character = pattern[index]
        if character == "*" and index + 1 < len(pattern) and pattern[index + 1] == "*":
            expression += ".*"
            index += 2
        elif character == "*":
            expression += "[^/]*"
            index += 1
        else:
            expression += re.escape(character)
            index += 1
    return re.fullmatch(expression, path) is not None


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} root must be an object")
    return value


def source_inventory(source: dict[str, Any]) -> list[dict[str, Any]] | None:
    components = source.get("components")
    if not isinstance(components, list) or not components:
        raise ValueError(f"{source.get('id')} has no components")
    evidence_path = components[0].get("evidence", {}).get("record")
    if not isinstance(evidence_path, str):
        raise ValueError(f"{source.get('id')} has no evidence record")
    evidence = load_json(ROOT / evidence_path)
    tree = evidence.get("tree_inventory")
    if tree is None:
        return None
    if not isinstance(tree, dict) or not isinstance(tree.get("path"), str):
        raise ValueError(f"{source.get('id')} tree receipt is malformed")
    raw = (ROOT / tree["path"]).read_bytes()
    if hashlib.sha256(raw).hexdigest() != tree.get("sha256"):
        raise ValueError(f"{source.get('id')} tree receipt digest disagrees")
    inventory = json.loads(raw)
    entries = inventory.get("entries")
    if not isinstance(entries, list) or not all(isinstance(entry, dict) and isinstance(entry.get("path"), str) for entry in entries):
        raise ValueError(f"{source.get('id')} tree entries are malformed")
    return entries


def report(lock: dict[str, Any]) -> dict[str, Any]:
    sources = lock.get("sources")
    if not isinstance(sources, list):
        raise ValueError("lock.sources must be a list")
    records: list[dict[str, Any]] = []
    scanned_entries = 0
    covered_entries = 0
    ambiguous_entries = 0
    for source in sources:
        source_id = source.get("id")
        if not isinstance(source_id, str):
            raise ValueError("source id is missing")
        entries = source_inventory(source)
        if entries is None:
            records.append({"source_id": source_id, "tree_status": "not-scanned-core-gated"})
            continue
        components = source["components"]
        component_rows = []
        matched_paths_by_component: dict[str, set[str]] = {}
        for component in components:
            matches = {
                entry["path"]
                for entry in entries
                if tree_glob_matches(component["path"], entry["path"])
            }
            matched_paths_by_component[component["id"]] = matches
            component_rows.append(
                {
                    "id": component["id"],
                    "path": component["path"],
                    "asset_kind": component["asset_kind"],
                    "disposition": component["disposition"],
                    "tree_match_count": len(matches),
                    "external_asset_reference": len(matches) == 0
                    and component["asset_kind"] in {"data", "model", "binary", "service"},
                }
            )
        counts = Counter(path for matches in matched_paths_by_component.values() for path in matches)
        total = len(entries)
        covered = len(counts)
        overlap = sum(1 for count in counts.values() if count > 1)
        scanned_entries += total
        covered_entries += covered
        ambiguous_entries += overlap
        records.append(
            {
                "source_id": source_id,
                "tree_status": "scanned",
                "entries": total,
                "entries_matched_by_selected_components": covered,
                "entries_unclassified_by_selected_components": total - covered,
                "entries_matching_multiple_components": overlap,
                "components": component_rows,
            }
        )
    return {
        "schema_version": 1,
        "scope": "Selected-component coverage over quarantined external tree inventories; not legal admission or runnable capability evidence.",
        "tree_entries": {
            "external_scanned": scanned_entries,
            "matched_by_selected_components": covered_entries,
            "unclassified_by_selected_components": scanned_entries - covered_entries,
            "matching_multiple_components": ambiguous_entries,
        },
        "sources": records,
        "non_claims": [
            "A matched source file remains quarantined.",
            "Adapt disposition is not an implementation, SessionActor, rebuilt-binary, CI, release, or live-provider proof.",
            "Unclassified entries are not implicitly approved or rejected.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    parser.add_argument("--check", action="store_true", help="fail if a selected component path overlaps another")
    args = parser.parse_args()
    try:
        value = report(load_json(args.lock))
        if args.check and value["tree_entries"]["matching_multiple_components"] != 0:
            raise ValueError("selected component paths overlap; disposition would be ambiguous")
        print(json.dumps(value, indent=2, sort_keys=True))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
