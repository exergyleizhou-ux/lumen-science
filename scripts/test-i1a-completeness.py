#!/usr/bin/env python3
"""I1-A tamper corpus: the completeness verifier must catch every closure break."""

from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "third_party/upstream-lock.v2.json"
VERIFIER = ROOT / "scripts/verify-i1a-completeness.py"


def run(lock: dict) -> tuple[int, str]:
    path = Path(tempfile.mkdtemp(prefix="i1a-lock-")) / "lock-tmp.json"
    path.write_text(json.dumps(lock), encoding="utf-8")
    proc = subprocess.run(
        [sys.executable, str(VERIFIER), "--lock", str(path)],
        check=False,
        capture_output=True,
        text=True,
    )
    return proc.returncode, proc.stdout + proc.stderr


def main() -> int:
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    results: list[tuple[str, bool, str]] = []

    def check(name: str, mutate, expect_fail: bool = True) -> None:
        candidate = copy.deepcopy(lock)
        mutate(candidate)
        code, output = run(candidate)
        good = (code != 0) if expect_fail else (code == 0)
        results.append((name, good, "" if good else f"exit={code}; {output.strip()[:200]}"))

    check("the real I1-A lock passes", lambda d: None, expect_fail=False)
    check("a missing transitive bridge is rejected", lambda d: d.pop("transitive_bridge"))
    check(
        "a wrong transitive bridge digest is rejected",
        lambda d: d["transitive_bridge"].__setitem__("sha256", "0" * 64),
    )
    check(
        "a source outside the disposition vocabulary is rejected",
        lambda d: d["sources"][0]["components"][0].__setitem__("disposition", "admitted"),
    )
    check(
        "an adapt component without matching reuse_mode is rejected",
        lambda d: d["sources"][0]["components"][0].__setitem__("reuse_mode", "none"),
    )
    check(
        "a missing tree_inventory reference is rejected",
        lambda d: d["sources"][0]["components"][0]["evidence"].__setitem__("record", ""),
    )
    check(
        "a double-dispositioned entry is rejected",
        lambda d: d["sources"][0]["components"].append(
            {
                "id": "dup",
                "path": d["sources"][0]["components"][0]["path"],
                "asset_kind": "code",
                "disposition": "adapt",
                "reuse_mode": "adapt",
                "rights_status": "verified",
                "execution_authority": "none",
                "evidence": d["sources"][0]["components"][0]["evidence"],
            }
        ),
    )
    check(
        "a malformed receipt is rejected",
        lambda d: d["sources"][0].__setitem__(
            "adapted_source_receipts",
            [{"source_id": "x"}],
        ),
    )

    passed = sum(1 for _, good, _ in results if good)
    print("test-i1a-completeness")
    for name, good, detail in results:
        print(f"  {'ok' if good else 'FAIL':<4}  {name}{': ' + detail if detail else ''}")
    print(f"\n{'OK' if passed == len(results) else 'FAIL'}: {passed}/{len(results)} passed")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
