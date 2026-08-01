#!/usr/bin/env python3
"""Tamper tests for the canonical Lumen consumer pin contract."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/verify-lumen-platform-pin.py"
PIN = ROOT / "third_party/lumen-platform-pin.v1.json"
SPEC = importlib.util.spec_from_file_location("platform_pin", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load platform pin verifier")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def active_pin() -> dict[str, object]:
    return {
        "schema_version": 1,
        "status": "active",
        "repository": "https://github.com/exergyleizhou-ux/lumen.git",
        "consumer": "lumen-science",
        "source": {
            "commit": "a" * 40,
            "canonical_main_commit": "a" * 40,
            "source_lock_sha256": "b" * 64,
            "r0_manifest_sha256": "c" * 64,
        },
        "platform_api": {"commit": "a" * 40, "semver": "1.0.0", "compatibility_manifest_sha256": "e" * 64, "public_adapter_compile_fixture": "lumen-platform-extension-fixture"},
        "verification": {"github_ci_run": "https://github.com/exergyleizhou-ux/lumen/actions/runs/123456", "ci_commit": "a" * 40, "binary_sha256": "f" * 64},
        "rollback": {"source_commit": "1" * 40, "platform_api_commit": "1" * 40},
    }


def main() -> int:
    draft = json.loads(PIN.read_text(encoding="utf-8"))
    valid = active_pin()
    incomplete = copy.deepcopy(valid)
    del incomplete["platform_api"]["compatibility_manifest_sha256"]
    same_rollback = copy.deepcopy(valid)
    same_rollback["rollback"]["source_commit"] = valid["source"]["commit"]
    api_from_another_source = copy.deepcopy(valid)
    api_from_another_source["platform_api"]["commit"] = "d" * 40
    stale_ci = copy.deepcopy(valid)
    stale_ci["verification"]["ci_commit"] = "d" * 40
    split_rollback = copy.deepcopy(valid)
    split_rollback["rollback"]["platform_api_commit"] = "2" * 40
    results = [
        ("checked-in draft is an explicit non-pass blocker", MODULE.validate(draft) == 2),
        ("complete active evidence is accepted", MODULE.validate(valid) == 0),
        ("active pin without compatibility evidence fails", _fails(incomplete)),
        ("active pin without distinct rollback fails", _fails(same_rollback)),
        ("API from another source commit fails", _fails(api_from_another_source)),
        ("CI for another source commit fails", _fails(stale_ci)),
        ("split rollback source/API pair fails", _fails(split_rollback)),
    ]
    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


def _fails(value: dict[str, object]) -> bool:
    try:
        MODULE.validate(value)
    except ValueError:
        return True
    return False


if __name__ == "__main__":
    raise SystemExit(main())
