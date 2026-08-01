#!/usr/bin/env python3
"""Validate capability intake records against exact v2 source receipts.

Records are source/adaptation evidence.  They cannot grant execution authority
or inflate their evidence level beyond what their own record proves.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "third_party/upstream-lock.v2.json"
DEFAULT_RECORD = ROOT / "third_party/capability-intake/jvogan-motif/seq-analyze.v1.json"
SHA256 = re.compile(r"^[0-9a-f]{64}$")


def require(value: bool, message: str) -> None:
    if not value:
        raise ValueError(message)


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"{path} root must be an object")
    return value


def validate(record: dict[str, Any], lock: dict[str, Any]) -> None:
    require(record.get("schema_version") == 1, "capability record schema_version must be 1")
    require(record.get("id") == "motif-seq-analyze-v1", "capability record has unexpected id")
    source = next((item for item in lock["sources"] if item["id"] == record.get("source_id")), None)
    require(source is not None, "capability record source is absent from v2 lock")
    require(record.get("source_commit") == source["exact_commit"], "capability record source commit disagrees with v2 lock")
    rights = record.get("rights")
    require(isinstance(rights, dict), "capability rights must be an object")
    require(rights.get("root_spdx") == source["root_license"]["spdx"] == "MIT", "capability MIT rights disagree with source lock")
    require(rights.get("reuse_mode") == "adapt", "capability must be an adaptation record")
    implementation = record.get("implementation")
    require(isinstance(implementation, dict), "capability implementation must be an object")
    require(implementation.get("authority") == "Rust Lumen SessionActor", "capability cannot name a second execution authority")
    require(implementation.get("network") == "denied" and implementation.get("process_execution") == "denied", "offline capability must deny network and process execution")
    require(implementation.get("artifact_commit") == "store-owned hashed artifacts only", "capability must use store-owned artifacts")
    receipt_path = ROOT / source["components"][0]["evidence"]["record"]
    receipt = load(receipt_path)
    inventory_path = ROOT / receipt["tree_inventory"]["path"]
    raw = inventory_path.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == receipt["tree_inventory"]["sha256"], "source inventory digest disagrees with receipt")
    inventory = json.loads(raw)
    indexed = {entry["path"]: entry["sha256"] for entry in inventory["entries"]}
    source_files = record.get("source_files")
    require(isinstance(source_files, list) and len(source_files) == 8, "capability must lock exactly eight Motif source files")
    paths = set()
    for item in source_files:
        require(isinstance(item, dict), "capability source file must be an object")
        path = item.get("path")
        digest = item.get("sha256")
        require(isinstance(path, str) and path.startswith("src/bio/"), "capability source file must be under Motif src/bio")
        require(SHA256.fullmatch(str(digest)) is not None and indexed.get(path) == digest, "capability source hash disagrees with exact tree inventory")
        require(path not in paths, "capability source file repeats a path")
        paths.add(path)
    source_text = (ROOT / implementation["path"]).read_text(encoding="utf-8")
    require(f'pub const MOTIF_COMMIT: &str = "{record["source_commit"]}";' in source_text, "seqbench commit disagrees with capability intake")
    require(all(f'"{path}".into()' in source_text for path in paths), "seqbench provenance omits an intake source path")
    evidence = record.get("evidence")
    require(isinstance(evidence, dict) and evidence.get("intake_level") == "E2", "intake record may only claim E2")
    require(isinstance(evidence.get("why_not_higher"), str) and len(evidence["why_not_higher"]) >= 50, "E2 record must state its non-claim")
    require(isinstance(record.get("next_gates"), list) and len(record["next_gates"]) >= 3, "capability record must list future admission gates")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--record", type=Path, default=DEFAULT_RECORD)
    parser.add_argument("--lock", type=Path, default=LOCK)
    args = parser.parse_args()
    try:
        validate(load(args.record), load(args.lock))
        print("PASS: Motif seq_analyze capability intake is exact-source E2 only")
    except (OSError, ValueError, json.JSONDecodeError, StopIteration) as error:
        print(f"FAIL: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
