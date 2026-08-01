#!/usr/bin/env python3
"""Validate the v2 nine-source intake lock without fetching or executing sources.

An active lock is only an E1/E2 source-intake result.  It cannot assert a
runnable capability, a rebuilt binary, CI, release, or live-provider proof.
Draft locks deliberately return exit code 2 so they cannot be mistaken for a
passing admission gate.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LOCK = ROOT / "third_party/upstream-lock.v2.json"
DEFAULT_FORBIDDEN = ROOT / "third_party/forbidden-paths.v2.json"
EXPECTED_SOURCE_IDS = {
    "snap-stanford-biomni",
    "jvogan-motif",
    "aipoch-open-science",
    "qzzqzzb-openclaudescience",
    "hust-ningkang-lab-bgc-prophet",
    "aurekaresearch-opendde",
    "ai4s-research-open-science",
    "exergyleizhou-ux-lumen",
    "exergyleizhou-ux-lumen-science",
}
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
ALLOWED_ASSET_KINDS = {
    "code",
    "skill",
    "data",
    "model",
    "binary",
    "service",
    "document",
}
ALLOWED_DISPOSITIONS = {
    "vendor",
    "adapt",
    "clean-room",
    "catalog-only",
    "quarantine",
    "reject-authority",
    "reject-license",
    "reject-data-model",
}
ALLOWED_REUSE_MODES = {"none", "vendor", "adapt", "clean-room", "catalog-only"}


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


def require_relative_glob(value: Any, label: str) -> str:
    require(isinstance(value, str) and value, f"{label} is missing")
    path = Path(value)
    require(not path.is_absolute(), f"{label} must be relative")
    require(".." not in path.parts, f"{label} escapes its source root")
    return value


def require_sha(value: Any, label: str, pattern: re.Pattern[str] = SHA256_RE) -> str:
    require(isinstance(value, str) and pattern.fullmatch(value) is not None, f"{label} is malformed")
    return value


def verify_policy(policy: Any) -> None:
    require(isinstance(policy, dict), "lock.policy must be an object")
    require(
        policy.get("canonical_execution_authority") == "Rust Lumen SessionActor",
        "lock must keep Rust Lumen SessionActor as canonical execution authority",
    )
    require(policy.get("external_runtime_authority") == "denied", "external runtime authority is not denied")
    require(policy.get("root_license_overrides_nested_license") is False, "root license must not override nested licenses")
    require(policy.get("unreviewed_asset_is_executable") is False, "unreviewed asset cannot be executable")
    require(policy.get("unreviewed_data_or_model_is_scientific_truth") is False, "unreviewed data/model cannot be scientific truth")
    require(policy.get("provider_or_billable_calls_during_intake") is False, "intake must not make provider or billable calls")


def verify_forbidden_paths(value: dict[str, Any]) -> dict[tuple[str, str], dict[str, Any]]:
    require(value.get("schema_version") == 2, "forbidden-paths schema_version must be 2")
    rules = value.get("rules")
    require(isinstance(rules, list) and rules, "forbidden-paths rules are empty")
    found: dict[tuple[str, str], dict[str, Any]] = {}
    for index, rule in enumerate(rules):
        label = f"forbidden-paths.rules[{index}]"
        require(isinstance(rule, dict), f"{label} must be an object")
        source_id = rule.get("source_id")
        require(isinstance(source_id, str) and source_id, f"{label}.source_id is missing")
        path = require_relative_glob(rule.get("path"), f"{label}.path")
        key = (source_id, path)
        require(key not in found, f"forbidden-paths repeats rule: {source_id}:{path}")
        require(rule.get("copy_forbidden") is True, f"{label} must forbid copying")
        require(isinstance(rule.get("clean_room_only"), bool), f"{label}.clean_room_only must be a bool")
        require(
            rule.get("required_disposition") in {"reject-license", "reject-authority"},
            f"{label}.required_disposition must reject license or authority",
        )
        require(isinstance(rule.get("reason"), str) and len(rule["reason"]) >= 20, f"{label}.reason is not substantive")
        found[key] = rule
    required = {
        ("qzzqzzb-openclaudescience", "skills/docx/**"),
        ("qzzqzzb-openclaudescience", "skills/pdf/**"),
        ("qzzqzzb-openclaudescience", "skills/pptx/**"),
        ("qzzqzzb-openclaudescience", "skills/xlsx/**"),
    }
    require(required.issubset(found), "forbidden-paths is missing a required proprietary OpenClaudeScience rule")
    require(
        ("ai4s-research-open-science", "runtime/skills/external/**") in found,
        "forbidden-paths is missing the required AI4S external-skill rule",
    )
    return found


def verify_component(
    component: Any,
    label: str,
    forbidden: dict[tuple[str, str], dict[str, Any]],
    source_id: str,
) -> None:
    require(isinstance(component, dict), f"{label} must be an object")
    component_id = component.get("id")
    require(isinstance(component_id, str) and component_id, f"{label}.id is missing")
    path = require_relative_glob(component.get("path"), f"{label}.path")
    asset_kind = component.get("asset_kind")
    require(asset_kind in ALLOWED_ASSET_KINDS, f"{label}.asset_kind is unsupported")
    disposition = component.get("disposition")
    require(disposition in ALLOWED_DISPOSITIONS, f"{label}.disposition is unsupported")
    reuse_mode = component.get("reuse_mode")
    require(reuse_mode in ALLOWED_REUSE_MODES, f"{label}.reuse_mode is unsupported")
    rights_status = component.get("rights_status")
    require(rights_status in {"verified", "restricted"}, f"{label}.rights_status is unsupported")
    require(component.get("execution_authority") == "none", f"{label} attempts to become an execution authority")
    evidence = component.get("evidence")
    require(isinstance(evidence, dict), f"{label}.evidence must be an object")
    require_sha(evidence.get("source_sha256"), f"{label}.evidence.source_sha256")
    require_relative_glob(evidence.get("record"), f"{label}.evidence.record")
    rule = forbidden.get((source_id, path))
    if rule is not None:
        require(disposition == rule["required_disposition"], f"{label} violates forbidden-path disposition")
        if rule["clean_room_only"]:
            require(reuse_mode == "clean-room", f"{label} violates clean-room-only restriction")
            require(rights_status == "restricted", f"{label} must remain rights-restricted")
        else:
            require(reuse_mode == "none", f"{label} violates no-copy restriction")
    if disposition in {"vendor", "adapt"}:
        require(rights_status == "verified", f"{label} cannot {disposition} without verified rights")
        require(reuse_mode == disposition, f"{label} reuse_mode must match disposition")
    if disposition == "clean-room":
        require(reuse_mode == "clean-room", f"{label} clean-room disposition requires clean-room reuse mode")
    if disposition.startswith("reject-") and rule is None:
        require(reuse_mode == "none", f"{label} rejected component cannot have a reuse mode")


def verify_evidence_record(source: dict[str, Any], record_path: str, label: str) -> None:
    path = ROOT / record_path
    evidence = load_json(path, f"{label} evidence record")
    require(evidence.get("schema_version") == 1, f"{label} evidence record schema_version must be 1")
    for field in ("source_id", "exact_commit", "archive_sha256"):
        expected = source["id"] if field == "source_id" else source[field]
        require(evidence.get(field) == expected, f"{label} evidence record {field} disagrees with lock")
    require(evidence.get("runtime_admission") == "none", f"{label} evidence record cannot admit a runtime")
    root_license = evidence.get("root_license")
    require(root_license == source["root_license"], f"{label} evidence record root license disagrees with lock")
    inventory = evidence.get("nested_license_inventory")
    require(isinstance(inventory, list) and inventory, f"{label} evidence record has no nested license inventory")
    encoded = json.dumps(inventory, separators=(",", ":"), ensure_ascii=True).encode("utf-8")
    actual_digest = hashlib.sha256(encoded).hexdigest()
    require(
        actual_digest == source["nested_license_scan"]["sha256"],
        f"{label} evidence record nested license inventory digest disagrees with lock",
    )
    tree_inventory = evidence.get("tree_inventory")
    if tree_inventory is not None:
        require(isinstance(tree_inventory, dict), f"{label} tree_inventory must be an object")
        inventory_path = require_relative_glob(tree_inventory.get("path"), f"{label} tree_inventory.path")
        require_sha(tree_inventory.get("sha256"), f"{label} tree_inventory.sha256")
        entry_count = tree_inventory.get("entry_count")
        require(isinstance(entry_count, int) and entry_count > 0, f"{label} tree_inventory.entry_count must be positive")
        raw_inventory = (ROOT / inventory_path).read_bytes()
        require(
            hashlib.sha256(raw_inventory).hexdigest() == tree_inventory["sha256"],
            f"{label} tree inventory digest disagrees with receipt",
        )
        inventory = json.loads(raw_inventory)
        require(inventory.get("source_id") == source["id"], f"{label} tree inventory source_id disagrees with receipt")
        require(inventory.get("exact_commit") == source["exact_commit"], f"{label} tree inventory commit disagrees with receipt")
        entries = inventory.get("entries")
        require(isinstance(entries, list) and len(entries) == entry_count, f"{label} tree inventory entry_count disagrees with receipt")
        require(all(isinstance(entry, dict) for entry in entries), f"{label} tree inventory has a malformed entry")
        require(
            all(entry.get("disposition") == "quarantine" and entry.get("execution_authority") == "none" for entry in entries),
            f"{label} tree inventory contains a non-quarantined or executable entry",
        )


def verify_source_records(
    lock: dict[str, Any],
    forbidden: dict[tuple[str, str], dict[str, Any]],
    *,
    active: bool,
    verify_evidence_records: bool,
) -> None:
    sources = lock.get("sources")
    require(isinstance(sources, list), "lock.sources must be a list")
    seen_sources: set[str] = set()
    seen_component_keys: set[tuple[str, str]] = set()
    for source_index, source in enumerate(sources):
        label = f"sources[{source_index}]"
        require(isinstance(source, dict), f"{label} must be an object")
        source_id = source.get("id")
        require(isinstance(source_id, str) and source_id, f"{label}.id is missing")
        require(source_id not in seen_sources, f"lock repeats source id: {source_id}")
        seen_sources.add(source_id)
        repository = source.get("repository")
        require(
            isinstance(repository, str) and repository.startswith("https://github.com/") and repository.endswith(".git"),
            f"{label}.repository must be an exact GitHub clone URL",
        )
        require_sha(source.get("exact_commit"), f"{label}.exact_commit", SHA_RE)
        require_sha(source.get("archive_sha256"), f"{label}.archive_sha256")
        rights_status = source.get("rights_status")
        require(rights_status in {"verified", "pending"}, f"{label}.rights_status is unsupported")
        source_gate_status = source.get("source_gate_status")
        require(
            source_gate_status in {"pass", "blocked-upstream-r0", "evidence-collected"},
            f"{label}.source_gate_status is unsupported",
        )
        if active:
            require(rights_status == "verified", f"{label}.rights_status must be verified for an active lock")
            require(source_gate_status == "pass", f"{label}.source_gate_status must pass for an active lock")
        root_license = source.get("root_license")
        require(isinstance(root_license, dict), f"{label}.root_license must be an object")
        require(isinstance(root_license.get("spdx"), str) and root_license["spdx"], f"{label}.root_license.spdx is missing")
        require_relative_glob(root_license.get("path"), f"{label}.root_license.path")
        require_sha(root_license.get("sha256"), f"{label}.root_license.sha256")
        nested = source.get("nested_license_scan")
        require(isinstance(nested, dict), f"{label}.nested_license_scan must be an object")
        require(nested.get("status") == "complete", f"{label}.nested_license_scan must be complete")
        require_sha(nested.get("sha256"), f"{label}.nested_license_scan.sha256")
        components = source.get("components")
        require(isinstance(components, list) and components, f"{label}.components is empty")
        component_ids: set[str] = set()
        record_paths: set[str] = set()
        for component_index, component in enumerate(components):
            component_label = f"{label}.components[{component_index}]"
            verify_component(component, component_label, forbidden, source_id)
            component_id = component["id"]
            path = component["path"]
            record_paths.add(component["evidence"]["record"])
            require(component_id not in component_ids, f"{label} repeats component id: {component_id}")
            component_ids.add(component_id)
            key = (source_id, path)
            require(key not in seen_component_keys, f"lock repeats component path: {source_id}:{path}")
            seen_component_keys.add(key)
        if verify_evidence_records:
            require(len(record_paths) == 1, f"{label} components must point to one canonical source evidence record")
            verify_evidence_record(source, next(iter(record_paths)), label)
    require(seen_sources == EXPECTED_SOURCE_IDS, "lock must contain exactly the nine expected source ids")
    for key in forbidden:
        require(key in seen_component_keys, f"lock is missing forbidden component inventory: {key[0]}:{key[1]}")


def verify_lock(
    lock: dict[str, Any], forbidden: dict[tuple[str, str], dict[str, Any]], *, verify_evidence_records: bool
) -> int:
    require(lock.get("schema_version") == 2, "lock schema_version must be 2")
    require(isinstance(lock.get("recorded_at"), str) and lock["recorded_at"], "lock.recorded_at is missing")
    verify_policy(lock.get("policy"))
    expected = lock.get("expected_source_ids")
    require(isinstance(expected, list), "lock.expected_source_ids must be a list")
    require(len(expected) == len(set(expected)), "lock.expected_source_ids repeats an id")
    require(set(expected) == EXPECTED_SOURCE_IDS, "lock.expected_source_ids must be the exact nine-source set")
    status = lock.get("status")
    require(status in {"draft", "active"}, "lock.status must be draft or active")
    verify_source_records(lock, forbidden, active=status == "active", verify_evidence_records=verify_evidence_records)
    if status == "draft":
        blocked_by = lock.get("blocked_by")
        require(
            isinstance(blocked_by, list) and blocked_by and all(isinstance(item, str) and len(item) >= 20 for item in blocked_by),
            "draft lock must list substantive blockers",
        )
        print("BLOCKED: upstream lock v2 has nine-source evidence but is not eligible for admission")
        return 2
    print("PASS: upstream lock v2 validates nine-source E1/E2 intake only")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    parser.add_argument("--forbidden-paths", type=Path, default=DEFAULT_FORBIDDEN)
    parser.add_argument("--skip-evidence-records", action="store_true")
    args = parser.parse_args()
    try:
        forbidden = verify_forbidden_paths(load_json(args.forbidden_paths, "forbidden-paths"))
        return verify_lock(
            load_json(args.lock, "upstream lock"),
            forbidden,
            verify_evidence_records=not args.skip_evidence_records,
        )
    except ValueError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
