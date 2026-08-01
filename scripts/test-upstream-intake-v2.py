#!/usr/bin/env python3
"""Focused contract and negative tests for the v2 intake verifier."""

from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parent.parent
FORBIDDEN = ROOT / "third_party/forbidden-paths.v2.json"
VERIFIER = ROOT / "scripts/verify-upstream-lock-v2.py"
SOURCE_IDS = [
    "snap-stanford-biomni",
    "jvogan-motif",
    "aipoch-open-science",
    "qzzqzzb-openclaudescience",
    "hust-ningkang-lab-bgc-prophet",
    "aurekaresearch-opendde",
    "ai4s-research-open-science",
    "exergyleizhou-ux-lumen",
    "exergyleizhou-ux-lumen-science",
]


def component(source_id: str, index: int, *, path: str | None = None) -> dict[str, Any]:
    restricted = path is not None
    return {
        "id": f"{source_id}-component-{index}",
        "path": path or "src/**",
        "asset_kind": "skill" if restricted else "code",
        "disposition": "reject-license" if restricted else "catalog-only",
        "reuse_mode": "clean-room" if restricted else "catalog-only",
        "rights_status": "restricted" if restricted else "verified",
        "execution_authority": "none",
        "evidence": {
            "source_sha256": f"{index:064x}",
            "record": f"third_party/capability-intake/{source_id}/component-{index}.json"
        }
    }


def active_lock() -> dict[str, Any]:
    sources: list[dict[str, Any]] = []
    for index, source_id in enumerate(SOURCE_IDS, start=1):
        components = [component(source_id, index)]
        if source_id == "qzzqzzb-openclaudescience":
            components.extend(
                component(source_id, index + offset, path=path)
                for offset, path in enumerate(
                    ["skills/docx/**", "skills/pdf/**", "skills/pptx/**", "skills/xlsx/**"],
                    start=1,
                )
            )
        if source_id == "ai4s-research-open-science":
            components.append(
                {
                    "id": f"{source_id}-opencode-profile",
                    "path": "runtime/opencode-profile/**",
                    "asset_kind": "code",
                    "disposition": "reject-authority",
                    "reuse_mode": "none",
                    "rights_status": "restricted",
                    "execution_authority": "none",
                    "evidence": {
                        "source_sha256": f"{index + 300:064x}",
                        "record": f"third_party/capability-intake/{source_id}/external-skills.json",
                    },
                }
            )
        if source_id == "aipoch-open-science":
            components.append(
                {
                    "id": f"{source_id}-mcp-client",
                    "path": "src/main/connectors/mcp-client-manager.ts",
                    "asset_kind": "code",
                    "disposition": "reject-authority",
                    "reuse_mode": "none",
                    "rights_status": "verified",
                    "execution_authority": "none",
                    "evidence": {
                        "source_sha256": f"{index + 400:064x}",
                        "record": f"third_party/capability-intake/{source_id}/mcp-client.json",
                    },
                }
            )
        sources.append(
            {
                "id": source_id,
                "repository": f"https://github.com/example/{source_id}.git",
                "exact_commit": f"{index:040x}",
                "archive_sha256": f"{index:064x}",
                "rights_status": "verified",
                "source_gate_status": "pass",
                "root_license": {"spdx": "MIT", "path": "LICENSE", "sha256": f"{index + 100:064x}"},
                "nested_license_scan": {"status": "complete", "sha256": f"{index + 200:064x}"},
                "components": components,
            }
        )
    return {
        "schema_version": 2,
        "status": "active",
        "recorded_at": "2026-08-01T00:00:00+08:00",
        "policy": {
            "canonical_execution_authority": "Rust Lumen SessionActor",
            "external_runtime_authority": "denied",
            "root_license_overrides_nested_license": False,
            "unreviewed_asset_is_executable": False,
            "unreviewed_data_or_model_is_scientific_truth": False,
            "provider_or_billable_calls_during_intake": False,
        },
        "expected_source_ids": SOURCE_IDS,
        "sources": sources,
    }


def run(lock: dict[str, Any]) -> subprocess.CompletedProcess[str]:
    with tempfile.TemporaryDirectory() as tmp:
        lock_path = Path(tmp) / "lock.json"
        lock_path.write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")
        return subprocess.run(
            [sys.executable, str(VERIFIER), "--lock", str(lock_path), "--forbidden-paths", str(FORBIDDEN), "--skip-evidence-records"],
            check=False,
            capture_output=True,
            text=True,
        )


def main() -> int:
    pristine = active_lock()
    results: list[tuple[str, bool, str]] = []

    def check(
        name: str,
        mutate: Callable[[dict[str, Any]], None] | None,
        expected_exit: int,
        needle: str,
    ) -> None:
        candidate = copy.deepcopy(pristine)
        if mutate is not None:
            mutate(candidate)
        proc = run(candidate)
        output = proc.stdout + proc.stderr
        good = proc.returncode == expected_exit and needle in output
        results.append((name, good, "" if good else f"exit={proc.returncode}; output={output.strip()[:240]!r}"))

    check("an active nine-source lock passes E1/E2 only", None, 0, "PASS: upstream lock v2")
    check(
        "a source cannot be duplicated",
        lambda lock: lock["sources"].append(copy.deepcopy(lock["sources"][0])),
        1,
        "repeats source id",
    )
    check(
        "a component cannot omit its exact-one disposition",
        lambda lock: lock["sources"][0]["components"][0].pop("disposition"),
        1,
        "disposition is unsupported",
    )
    check(
        "a source component cannot become an execution authority",
        lambda lock: lock["sources"][0]["components"][0].__setitem__("execution_authority", "upstream runtime"),
        1,
        "attempts to become an execution authority",
    )
    check(
        "a proprietary path cannot be relabelled as adaptable",
        lambda lock: lock["sources"][3]["components"][1].update({"disposition": "adapt", "reuse_mode": "adapt", "rights_status": "verified"}),
        1,
        "violates forbidden-path disposition",
    )
    check(
        "an AI4S OpenCode profile cannot be relabelled as adaptable",
        lambda lock: lock["sources"][6]["components"][1].update({"disposition": "adapt", "reuse_mode": "adapt", "rights_status": "verified"}),
        1,
        "violates forbidden-path disposition",
    )
    check(
        "an AIPOCH Node MCP client cannot be relabelled as adaptable",
        lambda lock: lock["sources"][2]["components"][1].update({"disposition": "adapt", "reuse_mode": "adapt"}),
        1,
        "violates forbidden-path disposition",
    )

    draft = copy.deepcopy(pristine)
    draft["status"] = "draft"
    draft["sources"][7]["source_gate_status"] = "blocked-upstream-r0"
    draft["blocked_by"] = ["I1-02 must collect immutable source and rights evidence before activation."]
    proc = run(draft)
    output = proc.stdout + proc.stderr
    results.append(("a draft lock reports BLOCKED rather than PASS", proc.returncode == 2 and "BLOCKED:" in output, "" if proc.returncode == 2 and "BLOCKED:" in output else f"exit={proc.returncode}; output={output.strip()[:240]!r}"))

    actual = subprocess.run(
        [sys.executable, str(VERIFIER), "--lock", str(ROOT / "third_party/upstream-lock.v2.json"), "--forbidden-paths", str(FORBIDDEN)],
        check=False,
        capture_output=True,
        text=True,
    )
    actual_output = actual.stdout + actual.stderr
    results.append(("the checked-in draft has nine matching evidence records", actual.returncode == 2 and "nine-source evidence" in actual_output, "" if actual.returncode == 2 and "nine-source evidence" in actual_output else f"exit={actual.returncode}; output={actual_output.strip()[:240]!r}"))

    passed = sum(1 for _, good, _ in results if good)
    print("test-upstream-intake-v2")
    for name, good, detail in results:
        print(f"  {'ok' if good else 'FAIL':<4}  {name}{': ' + detail if detail else ''}")
    print(f"\n{'OK' if passed == len(results) else 'FAIL'}: {passed}/{len(results)} passed")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
