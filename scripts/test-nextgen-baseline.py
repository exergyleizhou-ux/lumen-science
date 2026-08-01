#!/usr/bin/env python3
"""Focused tamper tests for the F0 baseline verifier."""

from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parent.parent
BASELINE = ROOT / "docs/science/5.0/NEXTGEN_BASELINE.json"
GATES = ROOT / "docs/science/5.0/NEXTGEN_GATE_REGISTRY.json"
VERIFIER = ROOT / "scripts/verify-nextgen-baseline.py"


def run(baseline: dict[str, Any], gates: dict[str, Any]) -> subprocess.CompletedProcess[str]:
    with tempfile.TemporaryDirectory() as tmp:
        tmp_root = Path(tmp)
        baseline_path = tmp_root / "baseline.json"
        gates_path = tmp_root / "gates.json"
        baseline_path.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")
        gates_path.write_text(json.dumps(gates, indent=2) + "\n", encoding="utf-8")
        return subprocess.run(
            [
                sys.executable,
                str(VERIFIER),
                "--science-repo",
                str(ROOT),
                "--baseline",
                str(baseline_path),
                "--gates",
                str(gates_path),
            ],
            check=False,
            capture_output=True,
            text=True,
        )


def main() -> int:
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    gates = json.loads(GATES.read_text(encoding="utf-8"))
    results: list[tuple[str, bool, str]] = []

    def check(
        name: str,
        mutate: Callable[[dict[str, Any], dict[str, Any]], None] | None,
        needle: str,
        expected_exit: int = 1,
    ) -> None:
        candidate_baseline = copy.deepcopy(baseline)
        candidate_gates = copy.deepcopy(gates)
        if mutate is not None:
            mutate(candidate_baseline, candidate_gates)
        proc = run(candidate_baseline, candidate_gates)
        output = proc.stdout + proc.stderr
        good = proc.returncode == expected_exit and needle in output
        results.append(
            (
                name,
                good,
                "" if good else f"exit={proc.returncode}; output={output.strip()[:240]!r}",
            )
        )

    check("the real F0 baseline passes", None, "PASS: NextGen F0 baseline", expected_exit=0)
    check(
        "a plan hash cannot drift silently",
        lambda value, _gates: value["plan_inputs"][0].__setitem__("sha256", "0" * 64),
        "baseline input hash drifted",
    )
    check(
        "the dirty Lumen observation cannot become a pin",
        lambda value, _gates: value["canonical_lumen_observation"].__setitem__("pin_eligible", True),
        "must not be pin eligible",
    )
    check(
        "a mandatory gate cannot disappear",
        lambda _value, registry: registry["gates"].pop(),
        "required exact gate set",
    )
    check(
        "a gate cannot use an invented state",
        lambda _value, registry: registry["gates"][0].__setitem__("status", "GREENISH"),
        "status is unsupported",
    )

    passed = sum(1 for _, good, _ in results if good)
    print("test-nextgen-baseline")
    for name, good, detail in results:
        print(f"  {'ok' if good else 'FAIL':<4}  {name}{': ' + detail if detail else ''}")
    print(f"\n{'OK' if passed == len(results) else 'FAIL'}: {passed}/{len(results)} passed")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
