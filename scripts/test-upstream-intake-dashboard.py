#!/usr/bin/env python3
"""Focused negative contract tests for the intake coverage dashboard."""

from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/report-upstream-intake-v2.py"
LOCK = ROOT / "third_party/upstream-lock.v2.json"


def run(lock: dict[str, object]) -> subprocess.CompletedProcess[str]:
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "lock.json"
        path.write_text(json.dumps(lock), encoding="utf-8")
        return subprocess.run([sys.executable, str(SCRIPT), "--lock", str(path), "--check"], check=False, text=True, capture_output=True)


def main() -> int:
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    good = run(lock)
    bad_lock = copy.deepcopy(lock)
    bad_lock["sources"][0]["components"][0]["execution_authority"] = "upstream runtime"
    bad = run(bad_lock)
    results = [
        ("checked-in dashboard reports draft evidence without product proof", good.returncode == 0 and '"runnable_from_intake": 0' in good.stdout),
        ("checked-in dashboard separates nine source scans from Core migration (all scanned after V0)", good.returncode == 0 and '"sources_scanned": 9' in good.stdout and '"entries_scanned": 12191' in good.stdout and '"external_sources_scanned": 7' in good.stdout and '"core_sources_scanned": 2' in good.stdout),
        ("draft dashboard rejects a component that claims runtime authority", bad.returncode == 1 and "cannot contain a runnable component" in bad.stdout),
    ]
    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
