#!/usr/bin/env python3
"""Tamper corpus for the granular gate registry v2 verifier."""

from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "docs/science/5.0/NEXTGEN_GATE_REGISTRY_V2.json"
VERIFIER = ROOT / "scripts/verify-nextgen-gates-v2.py"


def run(registry: dict[str, Any]) -> subprocess.CompletedProcess[str]:
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "registry.json"
        path.write_text(json.dumps(registry, indent=2) + "\n", encoding="utf-8")
        return subprocess.run(
            [sys.executable, str(VERIFIER), "--registry", str(path)],
            check=False,
            capture_output=True,
            text=True,
        )


def main() -> int:
    registry = json.loads(REGISTRY.read_text(encoding="utf-8"))
    results: list[tuple[str, bool, str]] = []

    def check(
        name: str,
        mutate: Callable[[dict[str, Any]], None] | None,
        needle: str,
        expected_exit: int = 1,
    ) -> None:
        candidate = copy.deepcopy(registry)
        if mutate is not None:
            mutate(candidate)
        proc = run(candidate)
        output = proc.stdout + proc.stderr
        good = proc.returncode == expected_exit and needle in output
        results.append(
            (
                name,
                good,
                "" if good else f"exit={proc.returncode}; output={output.strip()[:240]!r}",
            )
        )

    def find(gates: list[dict[str, Any]], gate_id: str) -> dict[str, Any]:
        return next(g for g in gates if g["id"] == gate_id)

    check("the real granular registry passes", None, "PASS: NextGen granular", expected_exit=0)
    check(
        "PASS without Science-side receipt is rejected",
        lambda reg: find(reg["gates"], "SCIENCE_PR_CI_GATE").__setitem__("receipt", []),
        "PASS without Science-side receipt",
    )
    check(
        "PASS_UPSTREAM without upstream receipt is rejected",
        lambda reg: (
            find(reg["gates"], "LUMEN_R0_SOURCE_GATE").__setitem__("status", "PASS_UPSTREAM"),
            find(reg["gates"], "LUMEN_R0_SOURCE_GATE").__setitem__("upstream_receipt", []),
        ),
        "PASS_UPSTREAM without upstream receipt",
    )
    check(
        "an invented status is rejected",
        lambda reg: find(reg["gates"], "SCIENCE_PR_CI_GATE").__setitem__("status", "GREENISH"),
        "status is unsupported",
    )
    check(
        "a required gate cannot disappear",
        lambda reg: reg["gates"].pop(),
        "misses required gates",
    )
    check(
        "a gate cannot depend on an unknown gate",
        lambda reg: find(reg["gates"], "PRODUCT_PROOF_GATE").__setitem__("requires", ["NOT_A_GATE"]),
        "requires unknown gate",
    )
    check(
        "a dependency cycle is rejected",
        lambda reg: find(reg["gates"], "PLATFORM_API_GATE")["requires"].append("SCIENCE_MACOS_GA_GATE"),
        "dependency cycle",
    )
    check(
        "a PASS gate cannot depend on an unresolved gate",
        lambda reg: (
            find(reg["gates"], "SCIENCE_MACOS_GA_GATE").__setitem__("status", "PASS"),
            find(reg["gates"], "SCIENCE_MACOS_GA_GATE").__setitem__(
                "receipt", ["signed package + install rollback"]
            ),
        ),
        "depends on unresolved gate",
    )

    passed = sum(1 for _, good, _ in results if good)
    print("test-nextgen-gates-v2")
    for name, good, detail in results:
        print(f"  {'ok' if good else 'FAIL':<4}  {name}{': ' + detail if detail else ''}")
    print(f"\n{'OK' if passed == len(results) else 'FAIL'}: {passed}/{len(results)} passed")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
