#!/usr/bin/env python3
"""Verify the F0 NextGen baseline and hard-gate registry without network access."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BASELINE = ROOT / "docs/science/5.0/NEXTGEN_BASELINE.json"
DEFAULT_GATES = ROOT / "docs/science/5.0/NEXTGEN_GATE_REGISTRY.json"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
REQUIRED_GATES = {
    "LUMEN_R0_SOURCE_GATE",
    "PLATFORM_API_GATE",
    "TASKTREE_GATE",
    "SOURCE_INTAKE_GATE",
    "PRODUCT_PROOF_GATE",
}
ALLOWED_GATE_STATUSES = {
    "NOT_STARTED",
    "IMPLEMENTING",
    "BLOCKED_UPSTREAM",
    "BLOCKED_CONTRACT",
    "BLOCKED",
    "PASS",
    "PASS_UPSTREAM",
}


def fail(message: str) -> None:
    raise ValueError(message)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def load_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        fail(f"cannot read {label}: {error}")
    except json.JSONDecodeError as error:
        fail(f"cannot parse {label}: {error}")
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def relative_path(value: str, label: str) -> Path:
    path = Path(value)
    require(value != "", f"{label} is empty")
    require(not path.is_absolute(), f"{label} must be relative")
    require(".." not in path.parts, f"{label} escapes the Science repository")
    return path


def verify_baseline(baseline: dict[str, Any], science_repo: Path) -> None:
    require(baseline.get("schema_version") == 1, "baseline schema_version must be 1")
    require(
        baseline.get("status") == "historical_snapshot",
        "baseline status must be historical_snapshot",
    )
    science = baseline.get("science")
    require(isinstance(science, dict), "baseline.science must be an object")
    source_commit = science.get("source_commit")
    require(
        isinstance(source_commit, str) and SHA_RE.fullmatch(source_commit) is not None,
        "baseline.science.source_commit must be a full SHA",
    )
    ancestor = subprocess.run(
        ["git", "-C", str(science_repo), "merge-base", "--is-ancestor", source_commit, "HEAD"],
        check=False,
        capture_output=True,
        text=True,
    )
    require(ancestor.returncode == 0, "baseline source_commit is not an ancestor of HEAD")

    inputs = baseline.get("plan_inputs")
    require(isinstance(inputs, list) and inputs, "baseline.plan_inputs is empty")
    seen_paths: set[str] = set()
    for index, item in enumerate(inputs):
        label = f"baseline.plan_inputs[{index}]"
        require(isinstance(item, dict), f"{label} must be an object")
        path_text = item.get("path")
        require(isinstance(path_text, str), f"{label}.path is missing")
        relative = relative_path(path_text, f"{label}.path")
        require(path_text not in seen_paths, f"baseline repeats plan input: {path_text}")
        seen_paths.add(path_text)
        expected = item.get("sha256")
        require(
            isinstance(expected, str) and SHA256_RE.fullmatch(expected) is not None,
            f"{label}.sha256 is malformed",
        )
        path = science_repo / relative
        require(path.is_file(), f"baseline input is missing: {path_text}")
        require(
            sha256(path) == expected,
            f"baseline input hash drifted: {path_text}",
        )

def verify_lumen_observation(lumen: dict[str, Any]) -> None:
    require(isinstance(lumen, dict), "baseline.canonical_lumen_observation is missing")
    require(lumen.get("observation_only") is True, "Lumen observation must remain read-only")
    # 2026-08-06 revision: the canonical Lumen main line is now a released,
    # receipt-backed source (v2.0.0/v2.1.0/v2.2.0 tuples).  pin_eligible may be
    # true only when an R0 receipt is present and well-formed; otherwise the
    # observation must stay not pin eligible (dirty or unproven upstream).
    pin_eligible = lumen.get("pin_eligible")
    require(isinstance(pin_eligible, bool), "pin_eligible must be a boolean observation")
    if pin_eligible is True:
        receipt = lumen.get("r0_receipt")
        require(isinstance(receipt, dict), "pin requires an r0 receipt object")
        require(
            isinstance(receipt.get("release_tags"), list)
            and receipt["release_tags"]
            and all(isinstance(t, str) and t for t in receipt["release_tags"]),
            "pin r0 receipt must list release tags",
        )
        for field in ("source_commit_a", "evidence_commit_b"):
            value = receipt.get(field)
            require(
                isinstance(value, str) and SHA_RE.fullmatch(value) is not None,
                f"pin r0 receipt {field} must be a full SHA",
            )
        require(
            isinstance(receipt.get("ci_green"), str) and receipt["ci_green"],
            "pin r0 receipt must record exact CI green evidence",
        )
        require(
            isinstance(receipt.get("observed_at"), str) and receipt["observed_at"],
            "pin r0 receipt must record an observation timestamp",
        )
    else:
        require(
            lumen.get("r0_receipt") is None,
            "a non-eligible observation must not carry an r0 receipt",
        )
    # This is an observation, not a source-pin criterion.  The book may become
    # tracked on an otherwise dirty branch; treating the former "untracked"
    # observation as a permanent invariant would make the baseline reject a
    # strictly more auditable upstream state.  pin_eligible is allowed only
    # with a valid r0_receipt (2026-08-06: canonical main is released).
    require(
        isinstance(lumen.get("nextgen_execution_book_tracked"), bool),
        "nextgen_execution_book_tracked must be a boolean observation",
    )
    for field in ("local_head", "origin_main"):
        value = lumen.get(field)
        require(
            isinstance(value, str) and SHA_RE.fullmatch(value) is not None,
            f"baseline.canonical_lumen_observation.{field} must be a full SHA",
        )


def verify_baseline_lumen(baseline: dict[str, Any]) -> None:
    verify_lumen_observation(baseline.get("canonical_lumen_observation"))


def verify_gates(gate_registry: dict[str, Any]) -> None:
    require(gate_registry.get("schema_version") == 1, "gate registry schema_version must be 1")
    gates = gate_registry.get("gates")
    require(isinstance(gates, list), "gate registry gates must be a list")
    ids: set[str] = set()
    for index, gate in enumerate(gates):
        label = f"gates[{index}]"
        require(isinstance(gate, dict), f"{label} must be an object")
        gate_id = gate.get("id")
        require(isinstance(gate_id, str) and gate_id, f"{label}.id is missing")
        require(gate_id not in ids, f"gate registry repeats id: {gate_id}")
        ids.add(gate_id)
        require(
            gate.get("status") in ALLOWED_GATE_STATUSES,
            f"{label}.status is unsupported",
        )
        status = gate.get("status")
        if status == "PASS":
            receipt = gate.get("receipt")
            require(
                isinstance(receipt, list) and receipt and all(isinstance(i, str) and i for i in receipt),
                f"{label}: PASS without Science-side receipt evidence",
            )
        if status == "PASS_UPSTREAM":
            receipt = gate.get("upstream_receipt")
            require(
                isinstance(receipt, list) and receipt and all(isinstance(i, str) and i for i in receipt),
                f"{label}: PASS_UPSTREAM without upstream receipt evidence",
            )
        require(
            isinstance(gate.get("owner"), str) and gate["owner"],
            f"{label}.owner is missing",
        )
        requires = gate.get("requires")
        require(
            isinstance(requires, list) and all(isinstance(item, str) and item for item in requires),
            f"{label}.requires is malformed",
        )
    require(ids == REQUIRED_GATES, "gate registry does not contain the required exact gate set")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--science-repo", type=Path, default=ROOT)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--gates", type=Path, default=DEFAULT_GATES)
    args = parser.parse_args()
    try:
        verify_baseline(load_json(args.baseline, "baseline"), args.science_repo)
        verify_baseline_lumen(load_json(args.baseline, "baseline"))
        verify_gates(load_json(args.gates, "gate registry"))
    except ValueError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print("PASS: NextGen F0 baseline and gate registry verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
