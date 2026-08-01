#!/usr/bin/env python3
"""Focused regression tests for the copied-Core anti-growth ownership gate."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
VERIFY = ROOT / "scripts/verify-science-core-ownership.py"
LOCK = ROOT / "third_party/science-core-ownership.v1.json"


def run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(VERIFY), *args], check=False, capture_output=True, text=True)


def main() -> int:
    results: list[tuple[str, bool]] = []
    results.append(("self-test detects an authority expansion", run("--self-test").returncode == 0))
    results.append(("checked-in observed surface matches its ownership lock", run().returncode == 0))
    with tempfile.TemporaryDirectory() as directory:
        changed = json.loads(LOCK.read_text(encoding="utf-8"))
        changed["authority_map_sha256"] = "0" * 64
        path = Path(directory) / "bad-lock.json"
        path.write_text(json.dumps(changed), encoding="utf-8")
        results.append(("stale or substituted ownership digest fails closed", run("--lock", str(path)).returncode != 0))
    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
