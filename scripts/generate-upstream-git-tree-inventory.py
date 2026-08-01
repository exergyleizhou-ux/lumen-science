#!/usr/bin/env python3
"""Turn a GitHub recursive-tree response into a no-content quarantine ledger.

Unlike the checkout inventory tool, this accepts only Git object metadata.  It
is intended for sources containing paths that cannot lawfully be retained
locally: no blob body is fetched, emitted, or written by this program.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


MODEL_SUFFIXES = {".ckpt", ".onnx", ".pt", ".pth", ".safetensors"}
DATA_SUFFIXES = {".csv", ".db", ".fasta", ".fastq", ".jsonl", ".parquet", ".tsv"}
BINARY_SUFFIXES = {".dylib", ".dll", ".exe", ".so"}


def asset_kind(path: str) -> str:
    parts = Path(path).parts
    if any(part.lower() in {"skill", "skills"} for part in parts):
        return "skill"
    suffix = Path(path).suffix.lower()
    if suffix in MODEL_SUFFIXES:
        return "model"
    if suffix in DATA_SUFFIXES:
        return "data"
    if suffix in BINARY_SUFFIXES:
        return "binary"
    if suffix in {".md", ".pdf", ".txt", ".doc", ".docx"}:
        return "document"
    return "code"


def inventory(tree: dict[str, Any]) -> list[dict[str, Any]]:
    nodes = tree.get("tree")
    if not isinstance(nodes, list):
        raise ValueError("Git tree response has no tree list")
    records: list[dict[str, Any]] = []
    for node in nodes:
        if not isinstance(node, dict):
            raise ValueError("Git tree has a malformed node")
        path = node.get("path")
        kind = node.get("type")
        sha = node.get("sha")
        if not isinstance(path, str) or not path or path.startswith("/") or ".." in Path(path).parts:
            raise ValueError("Git tree has an unsafe path")
        if kind not in {"blob", "tree", "commit"} or not isinstance(sha, str) or len(sha) != 40:
            raise ValueError(f"Git tree has malformed metadata for {path}")
        if kind != "blob":
            continue
        record: dict[str, Any] = {
            "path": path,
            "kind": "git-blob-metadata-only",
            "git_blob_sha": sha,
            "candidate_asset_kind": asset_kind(path),
            "disposition": "quarantine",
            "execution_authority": "none",
        }
        if isinstance(node.get("size"), int):
            record["size_bytes"] = node["size"]
        records.append(record)
    records.sort(key=lambda record: record["path"])
    if not records:
        raise ValueError("Git tree contains no blob metadata")
    return records


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-id", required=True)
    parser.add_argument("--exact-commit", required=True)
    parser.add_argument("--recorded-at", required=True)
    parser.add_argument("--tree-json", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        source = json.loads(args.tree_json.read_text(encoding="utf-8"))
        if not isinstance(source, dict):
            raise ValueError("Git tree response root must be an object")
        entries = inventory(source)
        document = {
            "schema_version": 1,
            "source_id": args.source_id,
            "exact_commit": args.exact_commit,
            "recorded_at": args.recorded_at,
            "scope": "Git tree metadata only; no source blob content was fetched or retained; every entry remains quarantined.",
            "entries": entries,
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}")
        return 1
    print(f"PASS: wrote {len(entries)} metadata-only quarantined entries for {args.source_id}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
