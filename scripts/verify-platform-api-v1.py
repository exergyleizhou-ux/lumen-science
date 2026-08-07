#!/usr/bin/env python3
"""X-C1 platform API v1 verifier: method catalog + compatibility manifest.

Validates the 7-method baseline catalog against the pinned canonical tuple,
and exposes validate_request() so consumers can fail closed on unknown
methods / missing fields / unknown fields before anything goes on the wire.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "docs/science/5.0/platform-api/v1-method-catalog.json"
MANIFEST = ROOT / "docs/science/5.0/platform-api/compatibility-manifest.json"

EXPECTED_METHODS = [
    "x.ai/science/run_csv",
    "x.ai/science/import_preview",
    "x.ai/science/connector_fetch",
    "x.ai/science/ssh_scp_fixture",
    "x.ai/science/goal_host_verify",
    "x.ai/governedTree/status",
    "x.ai/governedTree/assignmentRecommendation",
]

# Per-method required fields as defined by canonical A=098f7cd4 serde structs.
EXPECTED_REQUIRED_FIELDS: dict[str, list[str]] = {
    "x.ai/science/run_csv": ["sessionId", "projectId", "ownerId", "storeRoot", "artifactRoot", "fixturePath"],
    "x.ai/science/import_preview": ["sessionId", "projectId", "ownerId", "storeRoot", "artifactRoot", "sourcePath"],
    "x.ai/science/connector_fetch": ["sessionId", "projectId", "ownerId", "storeRoot", "artifactRoot", "connectorId", "query", "fixturePaths"],
    "x.ai/science/ssh_scp_fixture": ["sessionId", "projectId", "ownerId", "storeRoot", "artifactRoot", "port", "hostKeySha256", "user", "identityFile", "knownHostsFile", "sshConfigFile", "direction", "localPath", "remotePath"],
    "x.ai/science/goal_host_verify": ["sessionId", "storeRoot", "runId"],
    "x.ai/governedTree/status": [],
    "x.ai/governedTree/assignmentRecommendation": [],
}

PIN_A = "098f7cd424c1015bfe0d1cbd88c96570b36064ca"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read {path.name}: {error}") from error
    require(isinstance(value, dict), f"{path.name} root must be an object")
    return value


def method_map(catalog: dict) -> dict[str, dict]:
    methods = catalog.get("methods")
    require(isinstance(methods, list) and methods, "catalog.methods must be a non-empty list")
    by_name: dict[str, dict] = {}
    for method in methods:
        require(isinstance(method, dict) and method.get("method"), "catalog method entry malformed")
        name = method["method"]
        require(name not in by_name, f"catalog repeats method {name}")
        by_name[name] = method
        for field in ("namespace", "side_effect_class", "required_fields", "unknown_field_policy", "status"):
            require(field in method and method[field] not in (None, ""), f"{name} missing {field}")
        require(
            isinstance(method["required_fields"], list),
            f"{name}.required_fields must be a list",
        )
    return by_name


def verify(catalog: dict, manifest: dict) -> None:
    require(catalog.get("schema_version") == 1, "catalog schema_version must be 1")
    require(catalog.get("api_version") == "v1", "catalog api_version must be v1")
    by_name = method_map(catalog)
    require(set(by_name) == set(EXPECTED_METHODS), "catalog method set drifted from the 7-method v1 baseline")
    for name, expected in EXPECTED_REQUIRED_FIELDS.items():
        require(
            by_name[name]["required_fields"] == expected,
            f"{name} required_fields drifted from canonical schema",
        )

    require(manifest.get("schema_version") == 1, "manifest schema_version must be 1")
    require(manifest.get("api_version") == "v1", "manifest api_version must be v1")
    composition = manifest.get("composition", {})
    require(
        composition.get("lumen_core_source_tuple", {}).get("source_commit_a") == PIN_A,
        "manifest must pin canonical source A=098f7cd4",
    )
    catalog_ref = manifest.get("catalog_ref", {})
    require(
        catalog_ref.get("path") == "docs/science/5.0/platform-api/v1-method-catalog.json",
        "manifest catalog_ref must name the v1 catalog",
    )
    rules = manifest.get("rules", {})
    require(
        rules.get("unknown_field_policy") == "reject — canonical params structs use serde deny_unknown_fields; consumers must not send fields absent from the catalog",
        "manifest unknown_field_policy must be reject",
    )
    require(
        rules.get("unknown_method_policy", "").startswith("reject"),
        "manifest unknown_method_policy must be reject",
    )


def validate_request(catalog: dict, method: str, params: dict) -> str | None:
    """Fail-closed consumer-side request validation.

    Returns None when the request is admissible, else a rejection reason.
    Never returns the request; callers must not proceed on a rejection.
    """
    by_name = method_map(catalog)
    entry = by_name.get(method)
    if entry is None:
        return f"unknown method: {method}"
    allowed = set(entry["required_fields"]) | set(entry.get("optional_fields", []))
    for field in params:
        if field not in allowed:
            return f"unknown field: {field}"
    for field in entry["required_fields"]:
        if params.get(field) in (None, ""):
            return f"missing required field: {field}"
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", type=Path, default=CATALOG)
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    args = parser.parse_args()
    try:
        verify(load_json(args.catalog), load_json(args.manifest))
    except ValueError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print("PASS: platform API v1 catalog + compatibility manifest verified (7 methods, pinned tuple)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
