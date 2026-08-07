#!/usr/bin/env python3
"""Create a deterministic, non-executable inventory of a checked-out source tree.

The inventory is deliberately a *discovery* artifact.  Every regular file is
initially quarantined, no symlink is followed, and a file larger than the
declared review budget aborts the run rather than silently disappearing.  A
separate rights/asset review must turn an entry into an upstream-lock component.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
from pathlib import Path
from typing import Any


CHUNK_SIZE = 1024 * 1024
MODEL_SUFFIXES = {".ckpt", ".onnx", ".pt", ".pth", ".safetensors"}
DATA_SUFFIXES = {".csv", ".db", ".fasta", ".fastq", ".jsonl", ".parquet", ".tsv"}
BINARY_SUFFIXES = {".dylib", ".dll", ".exe", ".so"}
SKILL_PATH_PARTS = {"skill", "skills"}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(CHUNK_SIZE), b""):
            digest.update(block)
    return digest.hexdigest()


def asset_kind(relative: Path) -> str:
    if any(part.lower() in SKILL_PATH_PARTS for part in relative.parts):
        return "skill"
    suffix = relative.suffix.lower()
    if suffix in MODEL_SUFFIXES:
        return "model"
    if suffix in DATA_SUFFIXES:
        return "data"
    if suffix in BINARY_SUFFIXES:
        return "binary"
    if suffix in {".md", ".pdf", ".txt", ".doc", ".docx"}:
        return "document"
    return "code"


def require_outside_source(source_root: Path, output: Path) -> None:
    try:
        output.resolve().relative_to(source_root.resolve())
    except ValueError:
        return
    raise ValueError("output must be outside source-root; never write an upstream checkout")


def inventory(source_id: str, source_root: Path, exact_commit: str, max_file_bytes: int) -> list[dict[str, Any]]:
    if not source_root.is_dir():
        raise ValueError("source-root must be an existing directory")
    records: list[dict[str, Any]] = []
    for path in sorted(source_root.rglob("*"), key=lambda item: item.relative_to(source_root).as_posix()):
        relative = path.relative_to(source_root)
        if relative.parts and relative.parts[0] == ".git":
            continue
        metadata = path.lstat()
        base: dict[str, Any] = {
            "path": relative.as_posix(),
            "candidate_asset_kind": asset_kind(relative),
            "disposition": "quarantine",
            "execution_authority": "none",
        }
        if stat.S_ISLNK(metadata.st_mode):
            base.update({"kind": "symlink", "link_target": os.readlink(path)})
        elif stat.S_ISREG(metadata.st_mode):
            if metadata.st_size > max_file_bytes:
                raise ValueError(f"file exceeds review budget ({max_file_bytes} bytes): {relative.as_posix()}")
            base.update({"kind": "file", "size_bytes": metadata.st_size, "sha256": sha256_file(path)})
        elif stat.S_ISDIR(metadata.st_mode):
            continue
        else:
            base.update({"kind": "special"})
        records.append(base)
    if not records:
        raise ValueError("source-root contains no inventory entries")
    return records


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-id", required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--exact-commit", required=True)
    parser.add_argument("--recorded-at", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--max-file-bytes", type=int, default=64 * 1024 * 1024)
    args = parser.parse_args()
    try:
        if args.max_file_bytes <= 0:
            raise ValueError("max-file-bytes must be positive")
        require_outside_source(args.source_root, args.output)
        entries = inventory(args.source_id, args.source_root, args.exact_commit, args.max_file_bytes)
        document = {
            "schema_version": 1,
            "source_id": args.source_id,
            "exact_commit": args.exact_commit,
            "recorded_at": args.recorded_at,
            "scope": "read-only tree inventory; every entry remains quarantined until separate rights and asset review",
            "entries": entries,
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except (OSError, ValueError) as error:
        print(f"FAIL: {error}")
        return 1
    print(f"PASS: wrote {len(entries)} quarantined inventory entries for {args.source_id}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
