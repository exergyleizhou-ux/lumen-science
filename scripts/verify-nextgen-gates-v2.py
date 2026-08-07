#!/usr/bin/env python3
"""Verify the granular NextGen gate registry v2 (2026-08-06 re-map).

Semantics:
- PASS_UPSTREAM records an upstream (canonical lumen) delivered capability with
  exact receipts; it is NOT a Science completion claim.
- PASS requires Science-side receipt evidence in this registry.
- No gate may claim PASS/PASS_UPSTREAM without a non-empty receipt field.
- Gate dependency edges must form an acyclic graph.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_REGISTRY = ROOT / "docs/science/5.0/NEXTGEN_GATE_REGISTRY_V2.json"

ALLOWED_STATUSES = {
    "NOT_STARTED",
    "IMPLEMENTING",
    "BLOCKED_UPSTREAM",
    "BLOCKED_CONTRACT",
    "BLOCKED",
    "FAILED",
    "PASS",
    "PASS_UPSTREAM",
    "DISABLED",
}

# Gates that must always exist in the granular registry (no silent deletion).
REQUIRED_GATES = {
    "SCIENCE_PR_CI_GATE",
    "SKILL_LIFECYCLE_AUTHORITY_GATE",
    "P0_NR_SAFETY_GATE",
    "LUMEN_R0_SOURCE_GATE",
    "PLATFORM_API_GATE",
    "TASKTREE_GATE",
    "CAPABILITY_GRANT_GATE",
    "TOOL_CONTRACT_GATE",
    "SECRET_BOUNDARY_GATE",
    "UNTRUSTED_CONTENT_GATE",
    "ACTIVITY_UNLOAD_GATE",
    "TREE_BUDGET_GATE",
    "OPERATION_RECOVERY_GATE",
    "WRITE_SCOPE_GATE",
    "FLOW_CONTROL_GATE",
    "LEDGER_REPLAY_GATE",
    "CONTEXT_MANIFEST_GATE",
    "NO_REPLAY_GATE",
    "ADVISOR_SHADOW_GATE",
    "HARNESS_REGRESSION_GATE",
    "BOUNDED_ASSIGNMENT_GATE",
    "KAIROS_LOCAL_GATE",
    "NG10_RELEASE_FOUNDATION_GATE",
    "UPDATER_TRUST_GATE",
    "SINGLE_BASE_GATE",
    "SOURCE_INTAKE_COMPLETENESS_GATE",
    "SOURCE_INTAKE_ACTIVE_GATE",
    "SCIENTIFIC_VALIDITY_GATE",
    "DEVICE_SAFETY_GATE",
    "SCIENCE_MACOS_GA_GATE",
    "PRODUCT_PROOF_GATE",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read gate registry: {error}") from error
    require(isinstance(value, dict), "gate registry root must be an object")
    return value


def verify(registry: dict) -> None:
    require(registry.get("schema_version") == 2, "gate registry schema_version must be 2")
    gates = registry.get("gates")
    require(isinstance(gates, list) and gates, "gate registry gates must be a non-empty list")

    by_id: dict[str, dict] = {}
    for index, gate in enumerate(gates):
        label = f"gates[{index}]"
        require(isinstance(gate, dict), f"{label} must be an object")
        gate_id = gate.get("id")
        require(isinstance(gate_id, str) and gate_id, f"{label}.id is missing")
        require(gate_id not in by_id, f"gate registry repeats id: {gate_id}")
        by_id[gate_id] = gate
        require(
            gate.get("status") in ALLOWED_STATUSES,
            f"{label}.status is unsupported",
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

    missing = REQUIRED_GATES - set(by_id)
    require(not missing, f"gate registry misses required gates: {sorted(missing)}")

    # Dependency DAG: every requires[] edge must reference an existing gate,
    # and the graph must be acyclic.
    for gate_id, gate in by_id.items():
        for dep in gate.get("requires", []):
            require(dep in by_id, f"gate {gate_id} requires unknown gate: {dep}")

    visiting: set[str] = set()
    visited: set[str] = set()

    def visit(node: str) -> None:
        require(node not in visiting, f"gate dependency cycle at {node}")
        if node in visited:
            return
        visiting.add(node)
        for dep in by_id[node].get("requires", []):
            visit(dep)
        visiting.remove(node)
        visited.add(node)

    for gate_id in by_id:
        visit(gate_id)

    # Honesty guard: a gate that is BLOCKED_UPSTREAM may not itself be a
    # required dependency of a PASS/PASS_UPSTREAM gate unless it has receipts
    # recorded (this keeps the registry from wiring around unresolved gates).
    # IMPLEMENTING counts as unresolved for this purpose: a PASS gate must not
    # stand on a dependency that is still being built.
    unresolved = {
        "BLOCKED",
        "BLOCKED_UPSTREAM",
        "BLOCKED_CONTRACT",
        "FAILED",
        "NOT_STARTED",
        "DISABLED",
        "IMPLEMENTING",
    }
    for gate_id, gate in by_id.items():
        if gate.get("status") in ("PASS", "PASS_UPSTREAM"):
            for dep in gate.get("requires", []):
                dep_gate = by_id[dep]
                if dep_gate.get("status") in unresolved:
                    raise ValueError(
                        f"gate {gate_id} depends on unresolved gate {dep} (status {dep_gate.get('status')})"
                    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--registry", type=Path, default=DEFAULT_REGISTRY)
    args = parser.parse_args()
    try:
        verify(load_json(args.registry))
    except ValueError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print("PASS: NextGen granular gate registry v2 verified (PASS/PASS_UPSTREAM carry receipts)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
