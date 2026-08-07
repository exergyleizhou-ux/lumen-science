#!/usr/bin/env python3
"""Report commit-to-commit duplication drift for xai-grok-science.

Unlike check-core-drift.py, this intentionally includes the Science domain
crate.  It reads Git tree objects at explicit revisions, so neither checkout
may be dirty and the result is not confused with a developer's worktree.
This is an inventory, not an admission lock or a single-Core completion claim.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


CRATE = "agent/crates/codegen/xai-grok-science"


class DriftError(RuntimeError):
    pass


def git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args], check=False, capture_output=True, text=True
    )
    if result.returncode:
        raise DriftError(result.stderr.strip() or result.stdout.strip() or "git command failed")
    return result.stdout


def resolve(repo: Path, rev: str) -> str:
    if not repo.is_dir():
        raise DriftError(f"repository does not exist: {repo}")
    return git(repo, "rev-parse", "--verify", f"{rev}^{{commit}}").strip()


def tree(repo: Path, rev: str) -> dict[str, str]:
    output = git(repo, "ls-tree", "-r", "-z", rev, "--", CRATE)
    entries: dict[str, str] = {}
    for raw in output.split("\0"):
        if not raw:
            continue
        try:
            meta, path = raw.split("\t", 1)
            _mode, kind, blob = meta.split(" ", 2)
        except ValueError as exc:
            raise DriftError(f"unexpected ls-tree entry: {raw!r}") from exc
        if kind != "blob" or not path.endswith(".rs"):
            continue
        if not path.startswith(CRATE + "/"):
            raise DriftError(f"crate path escaped prefix: {path}")
        entries[path[len(CRATE) + 1 :]] = blob
    if not entries:
        raise DriftError(f"no Rust files found under {CRATE} at {rev}")
    return entries


def report(science: dict[str, str], upstream: dict[str, str]) -> dict[str, Any]:
    shared = sorted(set(science) & set(upstream))
    identical = sorted(path for path in shared if science[path] == upstream[path])
    diverged = sorted(path for path in shared if science[path] != upstream[path])
    science_only = sorted(set(science) - set(upstream))
    upstream_only = sorted(set(upstream) - set(science))
    entries: list[dict[str, str]] = []
    for kind, paths in (("shared_diverged", diverged), ("science_only", science_only), ("upstream_only", upstream_only)):
        for path in paths:
            value = {"classification": kind, "path": path}
            if path in science:
                value["science_blob"] = science[path]
            if path in upstream:
                value["upstream_blob"] = upstream[path]
            entries.append(value)
    digest = hashlib.sha256(
        json.dumps(entries, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    return {
        "schema": "lumen-science-crate-drift-v1",
        "crate": CRATE,
        "science_files": len(science),
        "upstream_files": len(upstream),
        "shared_identical": len(identical),
        "shared_diverged": len(diverged),
        "science_only": len(science_only),
        "upstream_only": len(upstream_only),
        "duplicate_delta": len(diverged) + len(science_only) + len(upstream_only),
        "manifest_sha256": digest,
        "shared_diverged_paths": diverged,
        "science_only_paths": science_only,
        "upstream_only_paths": upstream_only,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--science-repo", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--science-rev", default="HEAD")
    parser.add_argument("--upstream-repo", type=Path, required=True)
    parser.add_argument("--upstream-rev", required=True)
    args = parser.parse_args()
    try:
        science_rev = resolve(args.science_repo, args.science_rev)
        upstream_rev = resolve(args.upstream_repo, args.upstream_rev)
        result = report(tree(args.science_repo, science_rev), tree(args.upstream_repo, upstream_rev))
        result["science_commit"] = science_rev
        result["upstream_commit"] = upstream_rev
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    except DriftError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
