#!/usr/bin/env bash
# M5 human gate: validate one real stranger completing the Lumen path in <=10 minutes.
# This script validates evidence; it never fabricates or records a passing session.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARTIFACT="${M5_ARTIFACT:-$ROOT/artifacts/readiness/M5-onboarding.json}"
EVIDENCE_ROOT="${M5_EVIDENCE_ROOT:-$ROOT/journal/artifacts}"

python3 - "$ARTIFACT" "$EVIDENCE_ROOT" <<'PY'
import hashlib
import json
import re
import sys
from datetime import datetime
from pathlib import Path

artifact = Path(sys.argv[1])
evidence_root = Path(sys.argv[2]).expanduser().resolve()
required_steps = {
    "install",
    "configure_deepseek_key",
    "edit_file",
    "tool_calls_visible",
    "security_feedback_visible",
    "verify_feedback_visible",
}

def fail(message: str) -> None:
    print(f"BLOCKED: M5 evidence invalid: {message}")
    raise SystemExit(1)

try:
    doc = json.loads(artifact.read_text())
except FileNotFoundError:
    fail(f"missing {artifact}")
except json.JSONDecodeError as exc:
    fail(f"invalid JSON: {exc}")

if not isinstance(doc, dict):
    fail("root must be an object")
if doc.get("schema_version") != 1 or doc.get("check_id") != "M5_10_min_stranger":
    fail("schema_version/check_id mismatch")
if doc.get("pass") is not True:
    fail("pass must be true only after the observed run")

participant = doc.get("participant") or {}
if participant.get("relationship") != "stranger_to_lumen":
    fail("participant.relationship must be stranger_to_lumen")
if participant.get("prior_lumen_use") is not False:
    fail("participant must have no prior Lumen use")
if not isinstance(participant.get("anonymous_id"), str) or not participant["anonymous_id"].strip():
    fail("participant.anonymous_id is required")

observer = doc.get("observer") or {}
if observer.get("confirmed") is not True or not isinstance(observer.get("role"), str) or not observer["role"].strip():
    fail("an observer must confirm the run")

def parse_time(field: str) -> datetime:
    value = doc.get(field)
    if not isinstance(value, str):
        fail(f"{field} is required")
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        fail(f"{field} must be RFC3339")

started = parse_time("started_at")
finished = parse_time("finished_at")
computed = int((finished - started).total_seconds())
elapsed = doc.get("elapsed_seconds")
if type(elapsed) is not int or not 1 <= elapsed <= 600:
    fail("elapsed_seconds must be 1..600")
if abs(computed - elapsed) > 2:
    fail("elapsed_seconds does not match timestamps")

steps = doc.get("steps")
if not isinstance(steps, dict) or set(steps) != required_steps:
    fail("steps must contain exactly the six required M5 steps")

for step_id in sorted(required_steps):
    step = steps[step_id]
    if not isinstance(step, dict) or step.get("pass") is not True:
        fail(f"{step_id}.pass must be true")
    rel = step.get("evidence_path")
    expected_sha = step.get("sha256")
    if not isinstance(rel, str) or not rel or Path(rel).is_absolute():
        fail(f"{step_id}.evidence_path must be relative to the evidence root")
    candidate = (evidence_root / rel).resolve()
    try:
        candidate.relative_to(evidence_root)
    except ValueError:
        fail(f"{step_id}.evidence_path escapes the evidence root")
    if not candidate.is_file():
        fail(f"{step_id} evidence file is missing")
    actual_sha = hashlib.sha256(candidate.read_bytes()).hexdigest()
    if not isinstance(expected_sha, str) or not re.fullmatch(r"[0-9a-f]{64}", expected_sha):
        fail(f"{step_id}.sha256 must be lowercase SHA-256")
    if actual_sha != expected_sha:
        fail(f"{step_id} evidence SHA-256 mismatch")

if doc.get("credential_redacted") is not True:
    fail("credential_redacted must be true")
serialized = json.dumps(doc, ensure_ascii=False)
if re.search(r"(?:sk|ds)-[A-Za-z0-9_-]{12,}", serialized):
    fail("artifact appears to contain a credential")

print(f"OK: M5 stranger path passed in {elapsed}s with six hashed evidence files")
PY
