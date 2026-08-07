#!/usr/bin/env python3
"""M1 single-base gate: the Science repo owns no Rust product copy.

Lumen is the one Rust base. This gate fails closed on any of:

1. `agent/` (or any other copied Core workspace) present in the Science repo;
2. a Science CI product-test job that builds its own composition root
   instead of consuming the pinned Lumen tuple;
3. a machine gate that still compares against a local copy instead of
   asserting zero-copy;
4. an admission lock whose pin is not the tuple the product tests consume.

The pin is read from the admission lock (single source of truth) and the
workflow file must reference it — X-U refreshes move the pin without
touching this gate.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "docs/science/5.0/core-v0.1.251-admission.lock.json"
CI_WORKFLOW = ROOT / ".github/workflows/science-ci.yml"

FORBIDDEN_COPY_DIRS = (
    "agent",
    "core",
    "rust-core",
)
COPIED_MANIFEST_GLOBS = (
    "Cargo.toml",
    "Cargo.lock",
)


def fail(message: str) -> None:
    raise ValueError(message)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def main() -> int:
    problems = []

    # 1. Zero-copy: no Rust product workspace outside packs/.
    for name in FORBIDDEN_COPY_DIRS:
        if (ROOT / name).exists():
            problems.append(
                f"copied Core directory {name}/ must not exist in the Science repo"
            )
    for manifest in ROOT.glob("*Cargo.toml"):
        if manifest.name in COPIED_MANIFEST_GLOBS:
            problems.append(f"stray Rust workspace manifest at {manifest.relative_to(ROOT)}")
    for manifest in ROOT.glob("*Cargo.lock"):
        problems.append(f"stray Rust lockfile at {manifest.relative_to(ROOT)}")

    # 2. Admission lock must exist and carry the pin.
    require(LOCK.exists(), f"admission lock missing: {LOCK}")
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    lumen_head = lock.get("comparison", {}).get("lumen_head")
    audited_head = lock.get("lumen_source", {}).get("audited_main_head")
    require(
        isinstance(lumen_head, str) and re.fullmatch(r"[0-9a-f]{40}", lumen_head),
        "admission lock comparison.lumen_head is malformed",
    )
    require(
        lumen_head == audited_head,
        "admission lock comparison.lumen_head must equal lumen_source.audited_main_head",
    )

    # 3. CI must consume the pinned tuple, never build its own copy. The pin is
    # read dynamically from the admission lock (single source of truth) so X-U
    # refreshes move it without touching the workflow.
    require(
        CI_WORKFLOW.exists(),
        f"science CI workflow missing: {CI_WORKFLOW}",
    )
    ci = CI_WORKFLOW.read_text(encoding="utf-8")
    require(
        "comparison']['lumen_head']" in ci or "comparison\\\"]['lumen_head" in ci
        or "comparison']['lumen_head']" in ci,
        "science CI must read the admission-lock pin (zero-copy consumption)",
    )
    require(
        "git clone" in ci and "lumen-pin" in ci,
        "science CI must checkout the pinned Lumen tuple for Rust work",
    )
    require(
        "cargo build -p xai-grok-pager-bin" in ci,
        "science CI must build the composition root from the pinned Lumen checkout",
    )
    require(
        "cargo test -p xai-grok-shell --test test_built_binary_e2e" in ci,
        "science CI must build the built-binary harness from the pinned Lumen checkout",
    )
    require(
        "GROK_BINARY" in ci,
        "science CI product tests must set GROK_BINARY to the pinned binary",
    )

    # 4. Machine gates must not compare against a local copy.
    gates = (ROOT / "scripts/science-machine-gates.sh").read_text(encoding="utf-8")
    for forbidden in (
        "check-core-drift.py --science-root",
        "verify-science-crate-drift.py",
        "test-science-core-ownership.py",
        "test-no-external-cargo-path-deps.py",
    ):
        if forbidden in gates:
            problems.append(
                f"machine gates still reference a copied-Core check: {forbidden}"
            )
    require(
        "verify-single-base.py" in gates,
        "machine gates must run the zero-copy verify-single-base.py gate",
    )

    if problems:
        for problem in problems:
            print(f"FAIL: {problem}", file=sys.stderr)
        return 1

    print(
        f"OK: zero-copy single base — no Rust copy in Science repo; "
        f"product tests consume pinned Lumen {lumen_head[:12]}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
