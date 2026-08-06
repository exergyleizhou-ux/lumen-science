#!/usr/bin/env python3
"""X-C1 tamper/negative corpus for the platform API v1 catalog.

Drives the real validate_request() of verify-platform-api-v1 (the shipped
function), plus structural negatives. The live seam negatives (against the
pinned lumen 2.2.0 binary) live in the companion tsx script
test-platform-api-live.mts.
"""

from __future__ import annotations

import copy
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "docs/science/5.0/platform-api/v1-method-catalog.json"
VERIFIER = ROOT / "scripts/verify-platform-api-v1.py"

import importlib.util

spec = importlib.util.spec_from_file_location("verify_platform_api_v1", VERIFIER)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)  # type: ignore[union-attr]

catalog = json.loads(CATALOG.read_text(encoding="utf-8"))


def main() -> int:
    results: list[tuple[str, bool, str]] = []

    def check(name: str, got: bool, detail: str = "") -> None:
        results.append((name, got, detail))

    # Real catalog path (positive controls).
    check(
        "known method with all required fields passes",
        module.validate_request(
            catalog,
            "x.ai/science/connector_fetch",
            {
                "sessionId": "s",
                "projectId": "p",
                "ownerId": "o",
                "storeRoot": "/tmp/store",
                "artifactRoot": "/tmp/art",
                "connectorId": "uniprot",
                "query": "P12345",
                "fixturePaths": ["/tmp/f.json"],
            },
        )
        is None,
    )
    check(
        "governedTree/status (no params) passes",
        module.validate_request(catalog, "x.ai/governedTree/status", {}) is None,
    )

    # Unknown-method negatives.
    for bad in [
        "x.ai/science/nonexistent",
        "x.ai/governedTree/nope",
        "x.ai/science/seq_analyze",  # exists only in the copied core, not in the v1 catalog
        "settings:list-skills",
        "x.ai/science/run_csv/extra",
    ]:
        reason = module.validate_request(catalog, bad, {})
        check(f"unknown method rejected: {bad}", reason is not None and "unknown method" in reason)

    # Missing-field negatives (drive real validate_request).
    check(
        "run_csv missing required field rejected",
        "missing required field: projectId"
        in (module.validate_request(catalog, "x.ai/science/run_csv", {"sessionId": "s"}) or ""),
    )
    check(
        "goal_host_verify missing runId rejected",
        "missing required field: runId"
        in (module.validate_request(catalog, "x.ai/science/goal_host_verify", {"sessionId": "s", "storeRoot": "/x"}) or ""),
    )

    # Unknown-field negatives (deny_unknown_fields mirror).
    check(
        "connector_fetch with unknown field rejected",
        "unknown field: token"
        in (
            module.validate_request(
                catalog,
                "x.ai/science/connector_fetch",
                {
                    "sessionId": "s",
                    "projectId": "p",
                    "ownerId": "o",
                    "storeRoot": "/s",
                    "artifactRoot": "/a",
                    "connectorId": "u",
                    "query": "q",
                    "fixturePaths": [],
                    "token": "secret",
                },
            )
            or ""
        ),
    )
    check(
        "ssh_scp_fixture with unknown field rejected",
        "unknown field: password"
        in (
            module.validate_request(catalog, "x.ai/science/ssh_scp_fixture", {"password": "x"})
            or ""
        ),
    )

    # Cross-version: a method that does not exist in api v1 is unknown.
    check(
        "cross-version method (future v2 name) rejected",
        module.validate_request(catalog, "x.ai/science/v2/run_csv", {}) is not None,
    )

    # Structural negatives against the verifier.
    def run_verifier(mutate) -> tuple[int, str]:
        tmp = copy.deepcopy(catalog)
        mutate(tmp)
        path = Path("/var/folders/dn/_prdhdnn5l53lb71bhtx_n5w0000gn/T/grok-goal-405d73baecdb/implementer/catalog-tmp.json")
        path.write_text(json.dumps(tmp), encoding="utf-8")
        proc = subprocess.run(
            [sys.executable, str(VERIFIER), "--catalog", str(path)],
            check=False,
            capture_output=True,
            text=True,
        )
        return proc.returncode, proc.stdout + proc.stderr

    code, _ = run_verifier(lambda c: c["methods"].pop())
    check("verifier rejects a catalog missing a baseline method", code != 0)
    code, _ = run_verifier(lambda c: c["methods"][0].__setitem__("required_fields", []))
    check("verifier rejects an entry with empty required_fields", code != 0)
    code, _ = run_verifier(lambda c: c["methods"][0].__setitem__("method", "x.ai/science/other"))
    check("verifier rejects a method-set drift", code != 0)

    # Consumer compile fixture must typecheck (strict) against the pinned tuple.
    fixture_tsconfig = (
        ROOT / "docs/science/5.0/platform-api/consumer-fixture/tsconfig.json"
    )
    proc = subprocess.run(
        [
            "npx",
            "tsc",
            "-p",
            str(fixture_tsconfig),
        ],
        cwd=ROOT / "packs/science-desktop",
        check=False,
        capture_output=True,
        text=True,
    )
    check("consumer compile fixture typechecks strict", proc.returncode == 0, proc.stdout + proc.stderr)

    passed = sum(1 for _, good, _ in results if good)
    print("test-platform-api-v1")
    for name, good, detail in results:
        print(f"  {'ok' if good else 'FAIL':<4}  {name}{': ' + detail if detail else ''}")
    print(f"\n{'OK' if passed == len(results) else 'FAIL'}: {passed}/{len(results)} passed")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
