#!/usr/bin/env bash
# R7 + source tuple reconcile: fail-closed semantic verification of release evidence.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
ART="$ROOT/artifacts/readiness"
mkdir -p "$ART"
cd "$ROOT"

echo "=== reconcile-evidence ==="

# Optionally refresh SOURCE_LOCK. Normal release verification creates the lock
# explicitly before any evidence so a dirty tree cannot be attributed to HEAD.
if [[ "${REFRESH_SOURCE_LOCK:-0}" == "1" ]] || [[ ! -f "$ROOT/SOURCE_LOCK.json" ]]; then
  "$ROOT/scripts/source-lock.sh"
fi

python3 - "$ROOT" "$ART" <<'PY'
import hashlib
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

root = Path(sys.argv[1])
art = Path(sys.argv[2])
now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
head = subprocess.check_output(
    ["git", "rev-parse", "HEAD"], text=True, cwd=root
).strip()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(path: Path, label: str, blockers: list[str]):
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        blockers.append(f"{label}_invalid_json:{type(exc).__name__}")
        return None
    if not isinstance(value, dict):
        blockers.append(f"{label}_invalid_json:root_not_object")
        return None
    return value


blockers: list[str] = []

# SOURCE_LOCK: exact HEAD plus the full configured critical-file content set.
lock_path = root / "SOURCE_LOCK.json"
lock = load_json(lock_path, "source_lock", blockers) if lock_path.is_file() else None
lock_sha = sha256(lock_path) if lock_path.is_file() else None
lock_head = ((lock or {}).get("monorepo") or {}).get("git_head") or ""
lock_head_match = lock_head == head
content_ok = True
mismatched: list[str] = []
critical = (lock or {}).get("critical_file_sha256") or {}
if not isinstance(critical, dict) or not critical:
    content_ok = False
    blockers.append("source_lock_critical_set_missing")
else:
    for rel, expected in critical.items():
        path = root / rel
        if not path.is_file():
            content_ok = False
            mismatched.append(f"{rel}:missing")
        elif not isinstance(expected, str) or sha256(path) != expected:
            content_ok = False
            mismatched.append(str(rel))
if not lock_head_match:
    blockers.append(
        f"source_lock_head_mismatch:lock={lock_head[:7] or '?'} head={head[:7]}"
    )
if not content_ok:
    blockers.append("source_lock_content_drift:" + ",".join(mismatched[:8]))
lock_fresh = lock_head_match and content_ok

# Binary tuple: both execution paths must be the same executable built from HEAD.
release_path = Path(
    os.environ.get("LUMEN_RELEASE_BIN", root / "agent/target/release/lumen")
)
installed_path = Path(
    os.environ.get("LUMEN_INSTALLED_BIN", Path.home() / ".local/bin/lumen")
)
release_sha = sha256(release_path) if release_path.is_file() else None
installed_sha = sha256(installed_path) if installed_path.is_file() else None
release_version = None
installed_version = None
for label, path in (("release", release_path), ("installed", installed_path)):
    if not path.is_file() or not os.access(path, os.X_OK):
        blockers.append(f"binary_missing:{label}:{path}")
        continue
    try:
        version = subprocess.check_output(
            [str(path), "--version"], text=True, stderr=subprocess.STDOUT
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        blockers.append(f"binary_version_failed:{label}:{type(exc).__name__}")
        continue
    if label == "release":
        release_version = version
    else:
        installed_version = version
if release_version and installed_version and release_version != installed_version:
    blockers.append("binary_version_mismatch")
if release_version and f"({head[:7]})" not in release_version:
    blockers.append(f"binary_head_mismatch:expected={head[:7]}")
if release_sha and installed_sha and release_sha != installed_sha:
    blockers.append("binary_sha256_mismatch")
binary_tuple_match = bool(
    release_sha
    and installed_sha
    and release_sha == installed_sha
    and release_version
    and release_version == installed_version
    and f"({head[:7]})" in release_version
)

# Required machine evidence and its semantics. L0 has no standalone artifact;
# its current-run result is verified from the preliminary status below.
artifact_specs = {
    "L1-tool-calls.json": {"L1"},
    "L2-min-e2e.json": {"L2"},
    "L3-multi-tool.json": {"L3"},
    "L4-fault-cancel.json": {"L4_full"},
    "L5-long-session.json": {"L5_full"},
    "R0-full.json": {"R0_full"},
    "R0-min.json": {"R0_min"},
    "eval-live.json": {"eval_live"},
}
artifact_docs: dict[str, dict] = {}
missing = [name for name in artifact_specs if not (art / name).is_file()]
if not (art / "status.json").is_file():
    missing.append("status.json")
if missing:
    blockers.append("readiness_missing:" + ",".join(sorted(missing)))

for name, expected_ids in artifact_specs.items():
    path = art / name
    if not path.is_file():
        continue
    doc = load_json(path, name.removesuffix(".json"), blockers)
    if doc is None:
        continue
    artifact_docs[name] = doc
    if doc.get("pass") is not True:
        blockers.append(f"artifact_not_pass:{name}")
    if doc.get("check_id") not in expected_ids:
        blockers.append(f"artifact_check_id_mismatch:{name}")

eval_doc = artifact_docs.get("eval-live.json")
if eval_doc is not None:
    count_fields = ("pass_count", "fail_count", "total", "min_required")
    if not all(type(eval_doc.get(key)) is int for key in count_fields):
        blockers.append("eval_live_invalid_counts")
    else:
        passed = eval_doc["pass_count"]
        failed = eval_doc["fail_count"]
        total = eval_doc["total"]
        minimum = eval_doc["min_required"]
        tasks = eval_doc.get("tasks")
        if (
            total != 20
            or passed + failed != total
            or minimum < 18
            or minimum > total
            or passed < minimum
            or eval_doc.get("silent_corruption") != 0
            or not isinstance(tasks, list)
            or len(tasks) != total
        ):
            blockers.append("eval_live_semantics_invalid")
    eval_binary = eval_doc.get("binary")
    if not isinstance(eval_binary, str) or Path(eval_binary).expanduser() != installed_path:
        blockers.append("eval_live_binary_path_mismatch")

# M5 is a human publish gate, not an automated engineering gate. If a passing
# artifact exists, validate its identity and core semantics; absence is allowed
# here because status.json carries the honest FAIL row until a real session is
# observed. This keeps reconciliation useful without fabricating human proof.
m5_path = art / "M5-onboarding.json"
m5_doc = load_json(m5_path, "M5-onboarding", blockers) if m5_path.is_file() else None
if m5_doc is not None:
    elapsed = m5_doc.get("elapsed_seconds")
    steps = m5_doc.get("steps")
    required_m5_steps = {
        "install", "configure_deepseek_key", "edit_file",
        "tool_calls_visible", "security_feedback_visible", "verify_feedback_visible",
    }
    m5_ok = (
        m5_doc.get("check_id") == "M5_10_min_stranger"
        and m5_doc.get("pass") is True
        and type(elapsed) is int
        and 1 <= elapsed <= 600
        and isinstance(steps, dict)
        and set(steps) == required_m5_steps
        and all(isinstance(row, dict) and row.get("pass") is True for row in steps.values())
    )
    if not m5_ok:
        blockers.append("m5_onboarding_semantics_invalid")

l4_doc = artifact_docs.get("L4-fault-cancel.json")
if l4_doc is not None:
    scenarios = l4_doc.get("scenarios")
    if l4_doc.get("scope") != "full_contract_short" or not isinstance(scenarios, dict):
        blockers.append("l4_full_semantics_invalid")
    else:
        expected = {
            "429": (3, 1),
            "500": (3, 1),
            "disconnect": (3, 1),
            "timeout": (1, 0),
            "cancel": (1, 0),
        }
        for name, (requests, effects) in expected.items():
            row = scenarios.get(name)
            if not isinstance(row, dict) or row.get("pass") is not True or row.get("agent_request_count") != requests or row.get("effect_count") != effects:
                blockers.append(f"l4_scenario_invalid:{name}")

l5_doc = artifact_docs.get("L5-long-session.json")
if l5_doc is not None:
    soak = l5_doc.get("soak") or {}
    l5_ok = (
        l5_doc.get("scope") == "full_contract_soak"
        and l5_doc.get("resume_same_session") is True
        and l5_doc.get("compaction_persisted") is True
        and l5_doc.get("cache_visible") is True
        and type(l5_doc.get("update_event_id_count")) is int
        and l5_doc["update_event_id_count"] > 0
        and l5_doc.get("update_event_ids_unique") is True
        and soak.get("executed") is True
        and type(soak.get("elapsed_seconds")) is int
        and soak["elapsed_seconds"] >= 3600
        and type(soak.get("resume_turns")) is int
        and soak["resume_turns"] > 0
    )
    if not l5_ok:
        blockers.append("l5_full_soak_semantics_invalid")

r0_full = artifact_docs.get("R0-full.json")
if r0_full is not None:
    if r0_full.get("binary_sha256") != release_sha:
        blockers.append("r0_full_binary_sha256_mismatch")
    r0_binary = r0_full.get("binary")
    if not isinstance(r0_binary, str) or Path(r0_binary).expanduser() != release_path:
        blockers.append("r0_full_binary_path_mismatch")

# Preliminary status is generated during this verifier run before reconcile.
status_path = art / "status.json"
status = load_json(status_path, "status", blockers) if status_path.is_file() else None
status_required = {
    "gitleaks",
    "binary_tuple_pre",
    "defaults",
    "security",
    "m2",
    "parity",
    "eval_harness",
    "verify_cli",
    "verticals",
    "L0_connect",
    "L1_tool_calls",
    "L2_min_e2e",
    "L3_multi_tool",
    "L4_fault_cancel",
    "L5_long_session",
    "L5_one_hour_soak",
    "R0_full",
    "R0_min",
    "sbom",
    "legal",
    "eval_live",
    "binary_tuple_post",
    "source_lock",
}
if status is not None:
    checks = status.get("checks")
    if not isinstance(checks, list):
        blockers.append("status_checks_invalid")
        checks = []
    by_id = {
        item.get("id"): item
        for item in checks
        if isinstance(item, dict) and isinstance(item.get("id"), str)
    }
    for check_id in sorted(status_required):
        check = by_id.get(check_id)
        if not check or check.get("result") != "PASS" or check.get("pass") is not True:
            blockers.append(f"status_check_not_pass:{check_id}")
    for human_id in ("M5_10_min_stranger", "M6_15_day_self_use"):
        check = by_id.get(human_id)
        if not check or check.get("result") not in {"PASS", "FAIL"}:
            blockers.append(f"status_human_gate_missing:{human_id}")
    m5_status = by_id.get("M5_10_min_stranger") or {}
    if m5_status.get("result") == "PASS" and m5_doc is None:
        blockers.append("m5_onboarding_pass_without_artifact")
    if status.get("source_lock_sha256") != lock_sha:
        blockers.append("status_source_lock_sha256_mismatch")
    if status.get("binary_sha256") != release_sha:
        blockers.append("status_binary_sha256_mismatch")

# SBOM must identify this exact source HEAD and this exact SOURCE_LOCK.
sbom_path = root / "SBOM.spdx.json"
sbom = load_json(sbom_path, "sbom", blockers) if sbom_path.is_file() else None
sbom_sha = sha256(sbom_path) if sbom_path.is_file() else None
sbom_git_head = None
sbom_lock_sha = None
if sbom is not None:
    refs = []
    for package in sbom.get("packages") or []:
        if isinstance(package, dict) and package.get("name") == "lumen":
            refs.extend(package.get("externalRefs") or [])
    git_heads = {
        ref.get("referenceLocator")
        for ref in refs
        if isinstance(ref, dict) and ref.get("referenceType") == "gitCommit"
    }
    if head not in git_heads or sbom.get("name") != f"lumen-{head[:7]}":
        blockers.append("sbom_git_head_mismatch")
    for annotation in sbom.get("annotations") or []:
        if not isinstance(annotation, dict):
            continue
        comment = annotation.get("comment")
        if not isinstance(comment, str):
            continue
        try:
            meta = json.loads(comment)
        except json.JSONDecodeError:
            continue
        if isinstance(meta, dict) and "monorepo_git_head" in meta:
            sbom_git_head = meta.get("monorepo_git_head")
            files = meta.get("file_sha256") or {}
            sbom_lock_sha = files.get("SOURCE_LOCK.json") if isinstance(files, dict) else None
            break
    if sbom_git_head != head:
        blockers.append("sbom_annotation_head_mismatch")
    if sbom_lock_sha != lock_sha:
        blockers.append("sbom_source_lock_sha256_mismatch")

legal = root / "LEGAL.md"
legal_ok = legal.is_file() and legal.stat().st_size > 200
if not legal_ok:
    blockers.append("legal_too_short")

# Stable evidence digests exclude self/status to avoid a hash cycle.
artifact_sha = {}
skip = {"reconcile.json", "status.json", "engineering_complete.json", "M6-productivity.json"}
for path in sorted(art.glob("*.json")):
    if path.name not in skip:
        artifact_sha[path.name] = sha256(path)

ok = len(blockers) == 0
rec = {
    "schema_version": 1,
    "check_id": "reconcile_evidence",
    "pass": ok,
    "generated_at": now,
    "monorepo_git_head": head,
    "source_lock_sha256": lock_sha,
    "source_lock_git_head": lock_head,
    "source_lock_fresh": lock_fresh,
    "source_lock_head_match": lock_head_match,
    "source_lock_content_ok": content_ok,
    "binary_path": str(release_path),
    "installed_binary_path": str(installed_path),
    "binary_sha256": release_sha,
    "installed_binary_sha256": installed_sha,
    "binary_version": release_version,
    "binary_tuple_match": binary_tuple_match,
    "artifact_sha256": artifact_sha,
    "artifact_semantics_ok": not any(
        item.startswith(("artifact_", "eval_live_", "readiness_missing:"))
        for item in blockers
    ),
    "status_semantics_ok": not any(item.startswith("status_") for item in blockers),
    "sbom_sha256": sbom_sha,
    "sbom_git_head": sbom_git_head,
    "sbom_source_lock_sha256": sbom_lock_sha,
    "sbom_semantics_ok": not any(item.startswith("sbom_") for item in blockers),
    "legal_present": legal_ok,
    "blockers": blockers,
    "note": "R7: current-run status and L0-L5/R0/eval evidence reconcile to exact source, binaries, SOURCE_LOCK, and SBOM",
}

out = art / "reconcile.json"
if out.is_file():
    previous = load_json(out, "previous_reconcile", []) or {}
    material_keys = [
        "pass",
        "monorepo_git_head",
        "source_lock_sha256",
        "source_lock_git_head",
        "source_lock_fresh",
        "source_lock_head_match",
        "source_lock_content_ok",
        "binary_path",
        "installed_binary_path",
        "binary_sha256",
        "installed_binary_sha256",
        "binary_version",
        "binary_tuple_match",
        "artifact_sha256",
        "artifact_semantics_ok",
        "status_semantics_ok",
        "sbom_sha256",
        "sbom_git_head",
        "sbom_source_lock_sha256",
        "sbom_semantics_ok",
        "legal_present",
        "blockers",
    ]
    if all(previous.get(key) == rec.get(key) for key in material_keys):
        print(f"pass={ok} (unchanged) source_lock_sha256={(lock_sha or 'none')[:12]}…")
        print(f"lock_fresh={lock_fresh} head_match={lock_head_match} binary_tuple={binary_tuple_match}")
        print(f"blockers={blockers or []}")
        print(f"kept {out}")
        raise SystemExit(0 if ok else 1)

out.write_text(json.dumps(rec, indent=2) + "\n")
print(f"pass={ok} source_lock_sha256={(lock_sha or 'none')[:12]}… binary_sha256={(release_sha or 'none')[:12]}…")
print(f"lock_fresh={lock_fresh} head_match={lock_head_match} binary_tuple={binary_tuple_match}")
print(f"blockers={blockers or []}")
print(f"wrote {out}")
raise SystemExit(0 if ok else 1)
PY
