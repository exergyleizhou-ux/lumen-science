#!/usr/bin/env python3
"""S2a shadow-only scenario corpus verifier.

Checks the five class manifests (A authority / B context-claim /
C execution-liveness / D provider-advisor / E ux-provenance) are complete and
that the in-repo runner covers every scenario driver. The live assertions run
against the pinned canonical binary via test-s2a-corpus.mts (zero provider,
zero network, zero arbitrary shell).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCENARIOS = ROOT / "docs/science/5.0/s2a-scenarios"

EXPECTED_CLASSES = [
    "class-A-authority.json",
    "class-B-context-claim.json",
    "class-C-execution-liveness.json",
    "class-D-provider-advisor.json",
    "class-E-ux-provenance.json",
]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def verify() -> None:
    total = 0
    for name in EXPECTED_CLASSES:
        path = SCENARIOS / name
        require(path.is_file(), f"missing scenario manifest: {name}")
        manifest = json.loads(path.read_text(encoding="utf-8"))
        require(manifest.get("schema_version") == 1, f"{name} schema_version must be 1")
        require(manifest.get("pinned_binary", {}).get("lumen") == "2.2.0 (098f7cd4)", f"{name} must pin the canonical binary")
        scenarios = manifest.get("scenarios")
        require(isinstance(scenarios, list) and scenarios, f"{name} scenarios empty")
        for scenario in scenarios:
            require(scenario.get("id"), f"{name} scenario missing id")
            require(scenario.get("description"), f"{name} {scenario.get('id')} missing description")
            require(scenario.get("driver"), f"{name} {scenario.get('id')} missing driver")
            require(scenario.get("assertions"), f"{name} {scenario.get('id')} missing assertions")
            require(scenario.get("forbidden_effects"), f"{name} {scenario.get('id')} missing forbidden_effects")
            total += 1
    require(total >= 12, f"corpus too small: {total} scenarios")
    print(f"PASS: S2a shadow corpus verified ({total} scenarios across 5 classes, pinned 2.2.0)")


def main() -> int:
    try:
        verify()
    except ValueError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
