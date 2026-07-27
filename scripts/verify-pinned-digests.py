#!/usr/bin/env python3
"""Every hardcoded digest in a workflow must be a reviewed, attributed value.

Why this exists
---------------
A pinned checksum is a security control only if the value is real. A plausible
64-hex string that nobody verified is worse than no pin at all: the download
fails closed in CI (so it looks like a broken build rather than a fabricated
control), and reviewers see a checksum and assume the artifact was verified.

This was not hypothetical — during LS5 work two digests were written into
workflow files without being derived from the artifact they claimed to pin.
Both were caught and replaced with values taken from the already-reviewed core
release chain, and this check exists so the next one cannot reach main.

Rule
----
Every 40- or 64-hex literal in `.github/workflows/*.yml` must appear in
`.github/pinned-digests.txt` together with what it pins and how it was
obtained. Action pins (`uses: owner/repo@<sha>`) are exempt: they are checked
by GitHub itself on every run and would otherwise swamp the ledger.

    python3 scripts/verify-pinned-digests.py

Stdlib only.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"
LEDGER = ROOT / ".github" / "pinned-digests.txt"

HEX = re.compile(r"\b[0-9a-f]{40}\b|\b[0-9a-f]{64}\b")
# `uses: owner/repo@<sha>` — verified by GitHub when it resolves the action.
ACTION_PIN = re.compile(r"uses:\s*[^\s@]+@[0-9a-f]{40}")


def main() -> int:
    if not WORKFLOWS.is_dir():
        print("FAIL: no .github/workflows directory", file=sys.stderr)
        return 2

    known: set[str] = set()
    if LEDGER.is_file():
        for line in LEDGER.read_text(encoding="utf-8").splitlines():
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            for match in HEX.findall(stripped):
                known.add(match)

    unattributed: list[str] = []
    checked = 0
    for path in sorted(WORKFLOWS.glob("*.yml")):
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            # Drop action pins before scanning, so only real artifact digests remain.
            scannable = ACTION_PIN.sub("", line)
            for digest in HEX.findall(scannable):
                checked += 1
                if digest not in known:
                    rel = path.relative_to(ROOT)
                    unattributed.append(f"{rel}:{lineno}  {digest}")

    if unattributed:
        print("FAIL: unattributed digest(s) in workflow files:", file=sys.stderr)
        for item in unattributed:
            print(f"  {item}", file=sys.stderr)
        print(
            f"\nAdd each to {LEDGER.relative_to(ROOT)} with what it pins and how it was\n"
            "obtained — or, if you cannot say where it came from, it is not a pin.",
            file=sys.stderr,
        )
        return 1

    print(f"OK: {checked} artifact digest(s) in workflows, all attributed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
