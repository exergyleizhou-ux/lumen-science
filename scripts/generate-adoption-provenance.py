#!/usr/bin/env python3
"""Derive the Open Science adoption provenance ledger (LS5-F0-02).

Why
---
`IMPORT_LEDGER.md` records the absorb at directory granularity, in prose, with
several rows still marked "Planned". It cannot answer the questions that
actually matter for a borrowed codebase:

  - which specific files came from upstream, and at which commit?
  - which are byte-for-byte copies, and which did we modify?
  - which files are ours, and therefore not upstream's to license?
  - did an "authority removed" claim actually change the bytes?

Those are checkable facts, so derive them instead of asserting them: hash every
shipped desktop source file against the corresponding blob in the pinned
upstream commit and classify the result.

This matters legally as well as technically. Apache-2.0 requires attribution
and a statement of changes; a ledger that says "Planned" states neither.

Usage
-----
    python3 scripts/generate-adoption-provenance.py --upstream /path/to/open-science
    python3 scripts/generate-adoption-provenance.py --stdout

Stdlib only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
OUT_PATH = ROOT / "docs" / "provenance" / "open-science-adoption.json"
DESKTOP_SRC = ROOT / "packs" / "science-desktop" / "src"

# Pinned upstream. Taken from IMPORT_LEDGER.md; the verifier re-checks that the
# commit still resolves in the configured clone, so a silently-moved pin fails.
UPSTREAM_REPO = "https://github.com/aipoch/open-science"
UPSTREAM_COMMIT = "d8f11e34314fdfa36f750cdb617af1cc2f30bace"
UPSTREAM_LICENSE = "Apache-2.0"

# packs/science-desktop/src/X maps to upstream src/X.
DEST_PREFIX = "packs/science-desktop/src"
UPSTREAM_PREFIX = "src"

SOURCE_SUFFIXES = {".ts", ".tsx", ".css", ".html"}

# Adoptions outside the desktop tree, where the path does not follow the
# src/X -> packs/science-desktop/src/X rule. Listed explicitly: an adoption the
# ledger cannot see is exactly what the ledger exists to prevent, and silence
# here would look identical to "we wrote it ourselves".
EXTRA_ADOPTIONS = {
    "agent/crates/codegen/xai-grok-science/resources/lumen_python_loop.py":
        "resources/notebook/python_loop.py",
}


class LedgerError(RuntimeError):
    pass


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def upstream_blobs(clone: Path, commit: str) -> dict[str, str]:
    """Map upstream path -> blob sha256, read from git at the pinned commit.

    Read from git rather than the working tree: the checkout may be dirty or on
    a different commit, and provenance must be anchored to the pin.
    """
    check = subprocess.run(
        ["git", "cat-file", "-t", commit], cwd=clone, capture_output=True, text=True
    )
    if check.returncode != 0 or check.stdout.strip() != "commit":
        raise LedgerError(
            f"pinned upstream commit {commit[:12]} not found in {clone} — "
            "clone the upstream repo or pass --upstream"
        )

    listing = subprocess.run(
        ["git", "ls-tree", "-r", "--name-only", commit],
        cwd=clone,
        capture_output=True,
        text=True,
        check=True,
    )
    all_paths = listing.stdout.splitlines()
    # The desktop tree, plus any explicitly-mapped adoption from elsewhere in
    # the upstream repo (EXTRA_ADOPTIONS). Without the second set the generator
    # cannot verify those claims and refuses to write the ledger — which is the
    # correct failure, but only if the paths are actually fetched.
    wanted = set(EXTRA_ADOPTIONS.values())
    paths = [p for p in all_paths if p.startswith(f"{UPSTREAM_PREFIX}/") or p in wanted]

    blobs: dict[str, str] = {}
    # Batch through cat-file to avoid one process per file.
    proc = subprocess.run(
        ["git", "cat-file", "--batch"],
        cwd=clone,
        # Binary mode (blobs are arbitrary bytes), so the request list must be
        # encoded too.
        input="\n".join(f"{commit}:{p}" for p in paths).encode(),
        capture_output=True,
        check=True,
    )
    out = proc.stdout
    offset = 0
    for path in paths:
        newline = out.index(b"\n", offset)
        header = out[offset:newline].decode()
        if "missing" in header:
            offset = newline + 1
            continue
        _, _, size_s = header.split()
        size = int(size_s)
        body = out[newline + 1 : newline + 1 + size]
        blobs[path] = sha256_bytes(body)
        offset = newline + 1 + size + 1  # trailing newline
    return blobs


def classify(dest_rel: str, local_sha: str, blobs: dict[str, str]) -> dict[str, Any]:
    upstream_path = dest_rel.replace(f"{DEST_PREFIX}/", f"{UPSTREAM_PREFIX}/", 1)
    upstream_sha = blobs.get(upstream_path)

    if upstream_sha is None:
        return {
            "origin": "lumen-original",
            "upstreamPath": None,
            "upstreamSha256": None,
            "localSha256": local_sha,
            "modified": None,
            # Not upstream's work, so upstream attribution does not apply to it.
            "noticeRequired": False,
        }
    modified = upstream_sha != local_sha
    return {
        "origin": "adopted-modified" if modified else "adopted-verbatim",
        "upstreamPath": upstream_path,
        "upstreamSha256": upstream_sha,
        "localSha256": local_sha,
        "modified": modified,
        # Apache-2.0 §4: attribution for adopted files, and modified files must
        # additionally carry a statement of changes.
        "noticeRequired": True,
    }


def build(clone: Path) -> dict[str, Any]:
    blobs = upstream_blobs(clone, UPSTREAM_COMMIT)

    entries: dict[str, Any] = {}
    for path in sorted(DESKTOP_SRC.rglob("*")):
        if not path.is_file() or path.suffix not in SOURCE_SUFFIXES:
            continue
        dest_rel = str(path.relative_to(ROOT))
        local_sha = sha256_bytes(path.read_bytes())
        entries[dest_rel] = classify(dest_rel, local_sha, blobs)

    # Explicitly-mapped adoptions from elsewhere in the tree.
    for dest_rel, upstream_path in sorted(EXTRA_ADOPTIONS.items()):
        path = ROOT / dest_rel
        if not path.is_file():
            raise LedgerError(
                f"EXTRA_ADOPTIONS names {dest_rel}, which does not exist. "
                "Remove the entry or restore the file — a ledger that points at "
                "nothing is worse than no entry."
            )
        local_sha = sha256_bytes(path.read_bytes())
        upstream_sha = blobs.get(upstream_path)
        if upstream_sha is None:
            raise LedgerError(
                f"upstream {upstream_path} not found at the pinned commit; "
                f"cannot substantiate the adoption claim for {dest_rel}"
            )
        entries[dest_rel] = {
            "origin": "adopted-modified" if upstream_sha != local_sha else "adopted-verbatim",
            "upstreamPath": upstream_path,
            "upstreamSha256": upstream_sha,
            "localSha256": local_sha,
            "modified": upstream_sha != local_sha,
            "noticeRequired": True,
        }

    counts = {"adopted-verbatim": 0, "adopted-modified": 0, "lumen-original": 0}
    for entry in entries.values():
        counts[entry["origin"]] += 1

    # Files upstream ships that we did not take. Recorded so "we adopted X" can
    # be distinguished from "we adopted all of X".
    taken = {e["upstreamPath"] for e in entries.values() if e["upstreamPath"]}
    not_adopted = sorted(
        p for p in blobs if p not in taken and Path(p).suffix in SOURCE_SUFFIXES
    )

    return {
        "schemaVersion": 1,
        "generator": "scripts/generate-adoption-provenance.py",
        "doc": (
            "Per-file provenance for source adopted from Open Science, derived by "
            "hashing each shipped file against the pinned upstream commit. "
            "Regenerate with: python3 scripts/generate-adoption-provenance.py"
        ),
        "upstream": {
            "repo": UPSTREAM_REPO,
            "commit": UPSTREAM_COMMIT,
            "license": UPSTREAM_LICENSE,
            "noticeFile": "third_party/open-science/NOTICE",
        },
        "summary": {
            **counts,
            "totalShipped": len(entries),
            "upstreamSourceFiles": sum(
                1 for p in blobs if Path(p).suffix in SOURCE_SUFFIXES
            ),
            "upstreamFilesNotAdopted": len(not_adopted),
        },
        "licenseObligations": {
            "attributionRequiredFor": counts["adopted-verbatim"] + counts["adopted-modified"],
            "statementOfChangesRequiredFor": counts["adopted-modified"],
            "note": (
                "Apache-2.0 section 4 requires retaining attribution for adopted files "
                "and stating that modified files were changed. Files classified "
                "lumen-original carry no upstream obligation."
            ),
        },
        "files": entries,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--upstream",
        type=Path,
        default=Path.home() / "code" / "lumen-open-science",
        help="path to a clone of the upstream repo containing the pinned commit",
    )
    parser.add_argument("--stdout", action="store_true")
    parser.add_argument("--out", type=Path, default=OUT_PATH)
    args = parser.parse_args()

    try:
        ledger = build(args.upstream)
    except LedgerError as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 2

    rendered = json.dumps(ledger, indent=2, ensure_ascii=False) + "\n"
    if args.stdout:
        sys.stdout.write(rendered)
        return 0

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(rendered, encoding="utf-8")
    s = ledger["summary"]
    print(f"OK: wrote {args.out.relative_to(ROOT)}")
    print(
        f"  verbatim={s['adopted-verbatim']} modified={s['adopted-modified']} "
        f"lumen-original={s['lumen-original']} of {s['totalShipped']} shipped"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
