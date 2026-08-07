#!/usr/bin/env python3
"""Validate the one-way Science consumer contract for canonical Lumen.

This deliberately validates evidence *references*, not a mutable local Lumen
checkout.  A draft pin returns 2; it is a visible blocker, never a pass.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PIN = ROOT / "third_party/lumen-platform-pin.v1.json"
SHA = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")
URL = re.compile(r"^https://github\.com/[^/]+/[^/]+/actions/runs/\d+$")


def require(value: bool, message: str) -> None:
    if not value:
        raise ValueError(message)


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(value, dict), "platform pin root must be an object")
    return value


def validate(pin: dict[str, Any]) -> int:
    require(pin.get("schema_version") == 1, "platform pin schema_version must be 1")
    require(pin.get("repository") == "https://github.com/exergyleizhou-ux/lumen.git", "platform pin must name canonical Lumen")
    require(pin.get("consumer") == "lumen-science", "platform pin consumer must be lumen-science")
    status = pin.get("status")
    require(status in {"draft", "active"}, "platform pin status must be draft or active")
    if status == "draft":
        blocked = pin.get("blocked_by")
        require(isinstance(blocked, list) and len(blocked) >= 2 and all(isinstance(item, str) and len(item) >= 30 for item in blocked), "draft platform pin must state both substantial blockers")
        non_claims = pin.get("non_claims")
        require(isinstance(non_claims, list) and len(non_claims) >= 3, "draft platform pin must preserve non-claims")
        print("BLOCKED: canonical Lumen source/API pin is not eligible for Science consumption")
        return 2
    source = pin.get("source")
    api = pin.get("platform_api")
    verification = pin.get("verification")
    rollback = pin.get("rollback")
    require(isinstance(source, dict), "active platform pin needs source evidence")
    require(isinstance(api, dict), "active platform pin needs public API evidence")
    require(isinstance(verification, dict), "active platform pin needs exact CI evidence")
    require(isinstance(rollback, dict), "active platform pin needs rollback evidence")
    require(SHA.fullmatch(str(source.get("commit"))) is not None, "source.commit must be a full SHA")
    require(
        source.get("canonical_main_commit") == source["commit"],
        "active source.commit must be the exact canonical main commit",
    )
    require(SHA256.fullmatch(str(source.get("source_lock_sha256"))) is not None, "source.source_lock_sha256 must be SHA-256")
    require(SHA256.fullmatch(str(source.get("r0_manifest_sha256"))) is not None, "source.r0_manifest_sha256 must be SHA-256")
    require(SHA.fullmatch(str(api.get("commit"))) is not None, "platform_api.commit must be a full SHA")
    require(
        api["commit"] == source["commit"],
        "platform API must be shipped by the exact Lumen source commit consumed by Science",
    )
    require(SEMVER.fullmatch(str(api.get("semver"))) is not None, "platform_api.semver must be SemVer")
    require(SHA256.fullmatch(str(api.get("compatibility_manifest_sha256"))) is not None, "platform_api.compatibility_manifest_sha256 must be SHA-256")
    require(isinstance(api.get("public_adapter_compile_fixture"), str) and len(api["public_adapter_compile_fixture"]) >= 10, "platform API needs a named public adapter compile fixture")
    require(URL.fullmatch(str(verification.get("github_ci_run"))) is not None, "verification.github_ci_run must be an exact GitHub Actions run URL")
    require(
        verification.get("ci_commit") == source["commit"],
        "exact GitHub CI must have tested the exact consumed source commit",
    )
    require(SHA256.fullmatch(str(verification.get("binary_sha256"))) is not None, "verification.binary_sha256 must be SHA-256")
    require(SHA.fullmatch(str(rollback.get("source_commit"))) is not None, "rollback.source_commit must be a full SHA")
    require(SHA.fullmatch(str(rollback.get("platform_api_commit"))) is not None, "rollback.platform_api_commit must be a full SHA")
    require(rollback["source_commit"] != source["commit"], "rollback source cannot equal active source")
    require(rollback["platform_api_commit"] != api["commit"], "rollback API cannot equal active API")
    require(
        rollback["platform_api_commit"] == rollback["source_commit"],
        "rollback API must be supplied by the same rollback source commit",
    )
    print("PASS: canonical Lumen source/API pin has complete Science consumer evidence")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pin", type=Path, default=DEFAULT_PIN)
    args = parser.parse_args()
    try:
        return validate(load(args.pin))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
