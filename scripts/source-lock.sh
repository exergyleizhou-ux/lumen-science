#!/usr/bin/env bash
# Refresh SOURCE_LOCK.json for the monorepo (FINAL-2.0 S0).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="/opt/homebrew/bin:$HOME/.local/bin:$PATH"

python3 <<'PY'
import hashlib, json, subprocess
from datetime import datetime, timezone
from pathlib import Path

root = Path(".")
head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
paths = [
    ".gitleaksignore",
    "agent/crates/codegen/xai-grok-shell-base/src/util/event_id.rs",
    "agent/crates/codegen/xai-grok-models/default_models.json",
    "agent/crates/codegen/lumen-guard/src/lib.rs",
    "agent/crates/codegen/lumen-discipline/src/lib.rs",
    "scripts/assert-defaults.sh",
    "scripts/check-binary-tuple.sh",
    "scripts/eval-coding.sh",
    "scripts/eval-coding-live.sh",
    "scripts/generate-sbom.sh",
    "scripts/install-local.sh",
    "scripts/onboarding-gate.sh",
    "scripts/productivity-gate.sh",
    "scripts/reconcile-evidence.sh",
    "scripts/source-lock.sh",
    "scripts/smoke-deepseek.sh",
    "scripts/smoke-deepseek-agent.sh",
    "scripts/smoke-deepseek-l2.sh",
    "scripts/smoke-deepseek-l3.sh",
    "scripts/smoke-deepseek-l4.sh",
    "scripts/smoke-deepseek-l5.sh",
    "scripts/smoke-r0-min.sh",
    "scripts/smoke-r0.sh",
    "scripts/smoke-verify.sh",
    "scripts/test-readiness-contract.sh",
    "scripts/test-onboarding-gate.sh",
    "scripts/verify-readiness.sh",
    "scripts/probe-local.sh",
    "scripts/doctor-verticals.sh",
    "docs/masterplan/09-FINAL-2.0-执行路径.md",
    "docs/masterplan/M5-onboarding-evidence.template.json",
    "docs/masterplan/00A-来源锁与运行合同.md",
]
files = {}
h = hashlib.sha256()
missing = []
for rel in paths:
    p = root / rel
    if not p.is_file():
        missing.append(rel)
        continue
    b = p.read_bytes()
    files[rel] = hashlib.sha256(b).hexdigest()
    h.update(rel.encode())
    h.update(b)
if missing:
    raise SystemExit("FAIL: critical source-lock files missing: " + ", ".join(missing))

lock = {
    "schema_version": 1,
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "monorepo": {"git_head": head, "git_short": head[:7]},
    "upstream_pin": {
        "doc": "agent/UPSTREAM.md",
        "source": "xai-org/grok-build (local Desktop pin)",
        "policy": "PINNED; security-only cherry-picks",
    },
    "masterplan_authority": {
        "desktop": "Lumen Masterplan FINAL-2.0 - 生产级执行方案.docx",
        "baseline": "Lumen Masterplan.docx FINAL-1.1",
        "repo": "docs/masterplan/",
    },
    "critical_file_sha256": files,
    "aggregate_critical_sha256": h.hexdigest(),
}
Path("SOURCE_LOCK.json").write_text(json.dumps(lock, indent=2) + "\n")
print("OK: wrote SOURCE_LOCK.json", head[:7])
PY
