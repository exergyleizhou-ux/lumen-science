#!/usr/bin/env python3
"""Fail closed if Science expands its copied-Core authority surface.

This gate deliberately freezes the complete source map, rather than counting
files or commands.  A same-count replacement is still an authority change and
must receive an explicit ownership-lock review.  It is not a claim that the
copy is canonical, safe, or ready to consume a Lumen Platform API.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LOCK = ROOT / "third_party/science-core-ownership.v1.json"
REPORT = ROOT / "scripts/report-science-authority-map.py"
PROJECTION_KEYS = (
    "files",
    "session_command_variants",
    "run_loop_arms",
    "session_actor_methods",
    "session_handle_methods",
    "acp_routes",
    "migration_rule",
)


def digest(value: dict[str, Any]) -> str:
    projection = {key: value[key] for key in PROJECTION_KEYS}
    canonical = json.dumps(projection, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()


def load_report() -> dict[str, Any]:
    result = subprocess.run(
        [sys.executable, str(REPORT), "--check"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise ValueError(f"authority source map failed: {result.stderr.strip() or result.stdout.strip()}")
    value = json.loads(result.stdout)
    if not isinstance(value, dict) or any(key not in value for key in PROJECTION_KEYS):
        raise ValueError("authority source map has an unsupported shape")
    return value


def validate(lock: dict[str, Any], observed: dict[str, Any]) -> None:
    if lock.get("schema_version") != 1 or lock.get("status") != "enforced":
        raise ValueError("ownership lock must be schema v1 with enforced status")
    expected = lock.get("authority_map_sha256")
    if not isinstance(expected, str) or len(expected) != 64:
        raise ValueError("ownership lock is missing its authority-map SHA-256")
    non_claims = lock.get("non_claims")
    if not isinstance(non_claims, list) or len(non_claims) < 3:
        raise ValueError("ownership lock must preserve its non-claims")
    if digest(observed) != expected:
        raise ValueError(
            "copied-Core authority surface changed; update the ownership lock only with a public-port migration or explicit reviewed exception"
        )


def self_test() -> None:
    base = {key: [] for key in PROJECTION_KEYS}
    base["migration_rule"] = "frozen"
    changed = dict(base)
    changed["acp_routes"] = ["x.ai/science/new_authority"]
    if digest(base) == digest(changed):
        raise ValueError("self-test did not distinguish an authority expansion")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
            print("PASS: ownership gate self-test")
            return 0
        lock = json.loads(args.lock.read_text(encoding="utf-8"))
        if not isinstance(lock, dict):
            raise ValueError("ownership lock root must be an object")
        validate(lock, load_report())
        print("PASS: copied-Core authority surface matches the enforced ownership lock")
        return 0
    except (OSError, ValueError, json.JSONDecodeError, KeyError) as error:
        print(f"FAIL: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
