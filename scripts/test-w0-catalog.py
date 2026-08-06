#!/usr/bin/env python3
"""W0-A tamper corpus: catalog state/visibility discipline."""

from __future__ import annotations

import copy
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "docs/science/5.0/w0-catalog/catalog.v1.json"
VERIFIER = ROOT / "scripts/verify-w0-catalog.py"


def run(catalog: dict) -> tuple[int, str]:
    path = Path("/var/folders/dn/_prdhdnn5l53lb71bhtx_n5w0000gn/T/grok-goal-405d73baecdb/implementer/catalog-tmp.json")
    path.write_text(json.dumps(catalog), encoding="utf-8")
    proc = subprocess.run(
        [sys.executable, str(VERIFIER), "--catalog", str(path)],
        check=False,
        capture_output=True,
        text=True,
    )
    return proc.returncode, proc.stdout + proc.stderr


def main() -> int:
    catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
    results: list[tuple[str, bool, str]] = []

    def check(name: str, mutate, expect_fail: bool = True) -> None:
        candidate = copy.deepcopy(catalog)
        mutate(candidate)
        code, output = run(candidate)
        good = (code != 0) if expect_fail else (code == 0)
        results.append((name, good, "" if good else f"exit={code}; {output.strip()[:200]}"))

    def first(c: dict) -> dict:
        return c["entries"][0]

    check("the real catalog passes", lambda c: None, expect_fail=False)
    check("an unsupported admission state is rejected", lambda c: first(c).__setitem__("admission_state", "Ready"))
    check(
        "an advanced state without receipt is rejected",
        lambda c: (
            first(c).__setitem__("admission_state", "Managed"),
            first(c).pop("receipt"),
        ),
    )
    check(
        "a Cataloged entry cannot be runnable",
        lambda c: (
            first(c).__setitem__("admission_state", "Cataloged"),
            first(c).__setitem__("runnable", True),
            first(c).pop("receipt", None),
        ),
    )
    check(
        "a Quarantined entry must be dev-diagnostic",
        lambda c: (
            first(c).__setitem__("admission_state", "Quarantined"),
            first(c).__setitem__("ui_label", "candidate"),
        ),
    )
    check("a missing id is rejected", lambda c: first(c).pop("id"))

    passed = sum(1 for _, good, _ in results if good)
    print("test-w0-catalog")
    for name, good, detail in results:
        print(f"  {'ok' if good else 'FAIL':<4}  {name}{': ' + detail if detail else ''}")
    print(f"\n{'OK' if passed == len(results) else 'FAIL'}: {passed}/{len(results)} passed")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
