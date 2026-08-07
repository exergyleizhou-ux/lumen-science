#!/usr/bin/env python3
"""Fail closed when the audited duplicated Science crate inventory changes."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
REPORT = ROOT / "scripts/report-science-crate-drift.py"
DEFAULT_AUDIT = ROOT / "docs/science/5.0/science-crate-drift-audit-2026-08-01.json"
FIELDS = (
    "schema",
    "crate",
    "upstream_commit",
    "science_files",
    "upstream_files",
    "shared_identical",
    "shared_diverged",
    "science_only",
    "upstream_only",
    "duplicate_delta",
    "manifest_sha256",
)


def fail(message: str) -> int:
    print(f"FAIL: {message}", file=sys.stderr)
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream-repo", type=Path, required=True)
    parser.add_argument("--upstream-rev", required=True)
    parser.add_argument("--audit", type=Path, default=DEFAULT_AUDIT)
    args = parser.parse_args()
    try:
        audit = json.loads(args.audit.read_text(encoding="utf-8"))
        if not isinstance(audit, dict) or audit.get("status") != "inventory_only_not_admission_lock":
            return fail("audit must be an inventory-only object")
        command = [
            sys.executable, str(REPORT), "--science-repo", str(ROOT), "--science-rev", "HEAD",
            "--upstream-repo", str(args.upstream_repo), "--upstream-rev", args.upstream_rev,
        ]
        result = subprocess.run(command, check=False, capture_output=True, text=True)
        if result.returncode:
            return fail(result.stderr.strip() or result.stdout.strip() or "drift report failed")
        observed: dict[str, Any] = json.loads(result.stdout)
        for field in FIELDS:
            if observed.get(field) != audit.get(field):
                return fail(
                    f"Science crate drift {field} mismatch: actual={observed.get(field)!r} "
                    f"audit={audit.get(field)!r}; update inventory only with reviewed migration evidence"
                )
        print("PASS: duplicated Science crate inventory matches audited manifest")
        return 0
    except (OSError, ValueError, json.JSONDecodeError) as error:
        return fail(str(error))


if __name__ == "__main__":
    raise SystemExit(main())
