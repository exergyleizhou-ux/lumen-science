#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/evidence"

python3 - "$TMP/good.json" "$TMP/evidence" <<'PY'
import hashlib, json, sys
from pathlib import Path
out, root = Path(sys.argv[1]), Path(sys.argv[2])
steps = {}
for name in ["install", "configure_deepseek_key", "edit_file", "tool_calls_visible", "security_feedback_visible", "verify_feedback_visible"]:
    path = root / f"{name}.txt"
    path.write_text(f"observed {name}\n")
    steps[name] = {"pass": True, "evidence_path": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
doc = {
    "schema_version": 1,
    "check_id": "M5_10_min_stranger",
    "pass": True,
    "participant": {"relationship": "stranger_to_lumen", "prior_lumen_use": False, "anonymous_id": "participant-001"},
    "observer": {"confirmed": True, "role": "facilitator"},
    "started_at": "2026-07-16T12:00:00Z",
    "finished_at": "2026-07-16T12:09:30Z",
    "elapsed_seconds": 570,
    "credential_redacted": True,
    "steps": steps,
}
out.write_text(json.dumps(doc, indent=2) + "\n")
PY

M5_ARTIFACT="$TMP/good.json" M5_EVIDENCE_ROOT="$TMP/evidence" "$ROOT/scripts/onboarding-gate.sh" >/dev/null

python3 - "$TMP/good.json" "$TMP/bad-time.json" <<'PY'
import json, sys
d=json.load(open(sys.argv[1])); d["elapsed_seconds"]=601
json.dump(d, open(sys.argv[2], "w"))
PY
if M5_ARTIFACT="$TMP/bad-time.json" M5_EVIDENCE_ROOT="$TMP/evidence" "$ROOT/scripts/onboarding-gate.sh" >/dev/null 2>&1; then
  echo "FAIL: >600 second session passed" >&2; exit 1
fi

printf 'tampered\n' >>"$TMP/evidence/install.txt"
if M5_ARTIFACT="$TMP/good.json" M5_EVIDENCE_ROOT="$TMP/evidence" "$ROOT/scripts/onboarding-gate.sh" >/dev/null 2>&1; then
  echo "FAIL: tampered evidence passed" >&2; exit 1
fi

echo "OK: onboarding gate contract tests passed"
