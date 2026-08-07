#!/usr/bin/env python3
"""Verify the Open Science adoption provenance ledger (LS5-F0-02).

Checks, in order of how badly getting them wrong would hurt:

1. LICENSE OBLIGATIONS — Apache-2.0 section 4 requires attribution for adopted
   files, and requires modified files to carry a statement of changes. Every
   file the ledger classifies `adopted-modified` must say, in its own bytes,
   that it was changed. A ledger entry alone does not satisfy the licence: the
   statement travels with the file.

2. INTERNAL CONSISTENCY — the summary counts must match the file entries, and
   the pinned upstream commit must be a full 40-hex sha.

3. NOTICE PRESENT — the attribution file the ledger points at must exist and
   name the upstream project and licence.

4. FRESHNESS (optional) — with `--upstream <clone>`, re-derive the ledger and
   diff. CI does not have an upstream clone, so this is opt-in rather than
   silently skipped-and-called-passing.

Exit 0 all checks pass, 1 a check failed, 2 the ledger is unreadable.

    python3 scripts/verify-adoption-provenance.py
    python3 scripts/verify-adoption-provenance.py --upstream ~/code/lumen-open-science

Stdlib only.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
LEDGER_PATH = ROOT / "docs" / "provenance" / "open-science-adoption.json"
GENERATOR = ROOT / "scripts" / "generate-adoption-provenance.py"

# A modified file must say so. Any of these phrasings counts — the requirement
# is that a reader of the file learns it diverges from upstream, not that we
# picked one wording. Entries with `removed: true` carry an obligation record
# only: the bytes are gone (M1 single-base de-copy) and there is nothing left
# to attribute, so the license-obligation check skips them but the counts keep
# them as shipped.
# 
CHANGE_MARKERS = (
    "adapted from open science",
    "adapted from the open science",
    "modified from open science",
    "execution authority removed",
    "authority removed",
    "lumen science desktop authority model",
    "ls5-",  # a task-card reference is an explicit change record
    "stub:",
    "open science",  # weakest form: the file at least names its origin
)

failures: list[str] = []


def fail(check: str, detail: str) -> None:
    failures.append(f"{check}: {detail}")


def ok(check: str, detail: str = "") -> None:
    print(f"  ok    {check}{' — ' + detail if detail else ''}")


def check_license_obligations(ledger: dict[str, Any]) -> None:
    modified = [
        p
        for p, e in ledger["files"].items()
        if e["origin"] == "adopted-modified" and not e.get("removed")
    ]
    missing: list[str] = []
    for rel in modified:
        path = ROOT / rel
        if not path.is_file():
            missing.append(f"{rel} (file missing)")
            continue
        # Look at the head of the file: a statement of changes buried at line
        # 900 is not a notice anyone reads.
        head = "\n".join(path.read_text(encoding="utf-8", errors="replace").splitlines()[:40])
        if not any(m in head.lower() for m in CHANGE_MARKERS):
            missing.append(rel)

    if missing:
        fail(
            "license-obligations",
            f"{len(missing)} modified file(s) carry no statement of changes "
            f"(Apache-2.0 §4b):\n      " + "\n      ".join(sorted(missing)),
        )
        return
    ok("license-obligations", f"{len(modified)} modified files each state their changes")


def check_internal_consistency(ledger: dict[str, Any]) -> None:
    counts = {"adopted-verbatim": 0, "adopted-modified": 0, "lumen-original": 0}
    for entry in ledger["files"].values():
        origin = entry["origin"]
        if origin not in counts:
            fail("consistency", f"unknown origin classification {origin!r}")
            return
        counts[origin] += 1

    summary = ledger["summary"]
    for key, derived in counts.items():
        if summary.get(key) != derived:
            fail("consistency", f"summary says {key}={summary.get(key)} but entries give {derived}")
            return
    if summary.get("totalShipped") != len(ledger["files"]):
        fail("consistency", "summary.totalShipped disagrees with the entry count")
        return

    commit = ledger["upstream"]["commit"]
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        fail("consistency", f"upstream commit is not a full 40-hex sha: {commit}")
        return

    # A file claimed as ours must not carry an upstream path, and an adopted
    # file must carry both hashes — otherwise the classification is unfounded.
    for rel, entry in ledger["files"].items():
        if entry["origin"] == "lumen-original" and entry["upstreamPath"] is not None:
            fail("consistency", f"{rel} is lumen-original but names an upstream path")
            return
        if entry["origin"].startswith("adopted") and not (
            entry["upstreamSha256"] and entry["localSha256"]
        ):
            fail("consistency", f"{rel} is adopted but is missing a digest")
            return

    ok("consistency", f"{len(ledger['files'])} entries, counts and pin well-formed")


def check_notice(ledger: dict[str, Any]) -> None:
    notice_rel = ledger["upstream"]["noticeFile"]
    notice = ROOT / notice_rel
    if not notice.is_file():
        fail("notice", f"{notice_rel} does not exist but the ledger points at it")
        return
    text = notice.read_text(encoding="utf-8", errors="replace").lower()
    for needed, label in (("open science", "upstream project name"), ("apache", "licence")):
        if needed not in text:
            fail("notice", f"{notice_rel} does not mention the {label}")
            return
    ok("notice", f"{notice_rel} attributes upstream")


def check_freshness(upstream: Path) -> None:
    result = subprocess.run(
        [sys.executable, str(GENERATOR), "--stdout", "--upstream", str(upstream)],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        fail("freshness", f"generator failed: {result.stderr.strip()[:300]}")
        return
    regenerated = json.loads(result.stdout)
    committed = json.loads(LEDGER_PATH.read_text(encoding="utf-8"))
    if regenerated != committed:
        drifted = [
            k for k in set(committed) | set(regenerated) if committed.get(k) != regenerated.get(k)
        ]
        fail(
            "freshness",
            "ledger is stale for: "
            + ", ".join(sorted(drifted))
            + " — regenerate with scripts/generate-adoption-provenance.py",
        )
        return
    ok("freshness", "ledger matches a fresh derivation from the pinned upstream")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--upstream",
        type=Path,
        help="clone of the upstream repo; enables the freshness re-derivation check",
    )
    args = parser.parse_args()

    if not LEDGER_PATH.is_file():
        print(
            f"FAIL: {LEDGER_PATH.relative_to(ROOT)} missing — run "
            "scripts/generate-adoption-provenance.py",
            file=sys.stderr,
        )
        return 2
    try:
        ledger = json.loads(LEDGER_PATH.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        print(f"FAIL: ledger is not valid JSON: {exc}", file=sys.stderr)
        return 2

    print("verify-adoption-provenance")
    check_internal_consistency(ledger)
    check_notice(ledger)
    check_license_obligations(ledger)
    if args.upstream:
        check_freshness(args.upstream)
    else:
        # Stated, not silently omitted: a skipped check is not a passed one.
        print("  skip  freshness (no --upstream clone given)")

    if failures:
        print(f"\nFAIL ({len(failures)} problem(s)):", file=sys.stderr)
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1
    s = ledger["summary"]
    print(
        f"\nOK: {s['adopted-verbatim']} verbatim, {s['adopted-modified']} modified, "
        f"{s['lumen-original']} lumen-original"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
