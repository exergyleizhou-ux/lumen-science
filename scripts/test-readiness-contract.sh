#!/usr/bin/env bash
# Deterministic tests for readiness/source/binary reconciliation. No live APIs.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/lumen-readiness-contract-XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

expect_fail() {
  local label="$1"
  shift
  if "$@" >"$TMP/$label.out" 2>&1; then
    fail "$label unexpectedly passed"
  fi
}

init_git() {
  local repo="$1"
  git -C "$repo" init -q
  git -C "$repo" config user.name "Readiness Test"
  git -C "$repo" config user.email "readiness@test.invalid"
}

echo "=== install-local dirty tracked tree ==="
INSTALL_ROOT="$TMP/install"
mkdir -p "$INSTALL_ROOT/scripts" "$INSTALL_ROOT/agent/target/release"
cp "$ROOT/scripts/install-local.sh" "$INSTALL_ROOT/scripts/install-local.sh"
printf 'tracked\n' >"$INSTALL_ROOT/tracked.txt"
printf 'agent/target/\n' >"$INSTALL_ROOT/.gitignore"
init_git "$INSTALL_ROOT"
git -C "$INSTALL_ROOT" add .gitignore scripts/install-local.sh tracked.txt
git -C "$INSTALL_ROOT" commit -qm base
INSTALL_SHORT="$(git -C "$INSTALL_ROOT" rev-parse --short=7 HEAD)"
printf '#!/bin/sh\necho "lumen test (%s)"\n' "$INSTALL_SHORT" \
  >"$INSTALL_ROOT/agent/target/release/lumen"
chmod +x "$INSTALL_ROOT/agent/target/release/lumen"
INSTALL_DEST="$TMP/install-dest"

LUMEN_SKIP_BUILD=1 LUMEN_INSTALL_DIR="$INSTALL_DEST" \
  "$INSTALL_ROOT/scripts/install-local.sh" >/dev/null

printf 'dirty\n' >>"$INSTALL_ROOT/tracked.txt"
expect_fail install_unstaged env LUMEN_SKIP_BUILD=1 LUMEN_INSTALL_DIR="$INSTALL_DEST" \
  "$INSTALL_ROOT/scripts/install-local.sh"
LUMEN_ALLOW_DIRTY=1 LUMEN_SKIP_BUILD=1 LUMEN_INSTALL_DIR="$INSTALL_DEST" \
  "$INSTALL_ROOT/scripts/install-local.sh" >/dev/null
git -C "$INSTALL_ROOT" restore tracked.txt

printf 'staged\n' >>"$INSTALL_ROOT/tracked.txt"
git -C "$INSTALL_ROOT" add tracked.txt
expect_fail install_staged env LUMEN_SKIP_BUILD=1 LUMEN_INSTALL_DIR="$INSTALL_DEST" \
  "$INSTALL_ROOT/scripts/install-local.sh"
git -C "$INSTALL_ROOT" restore --staged tracked.txt
git -C "$INSTALL_ROOT" restore tracked.txt

printf 'untracked\n' >"$INSTALL_ROOT/scratch.txt"
expect_fail install_untracked env LUMEN_SKIP_BUILD=1 LUMEN_INSTALL_DIR="$INSTALL_DEST" \
  "$INSTALL_ROOT/scripts/install-local.sh"
rm "$INSTALL_ROOT/scratch.txt"

echo "=== self-update always rejects dirty source ==="
SELF_UPDATE_ROOT="$TMP/self-update"
mkdir -p "$SELF_UPDATE_ROOT/scripts"
cp "$ROOT/scripts/self-update.sh" "$SELF_UPDATE_ROOT/scripts/self-update.sh"
chmod +x "$SELF_UPDATE_ROOT/scripts/self-update.sh"
init_git "$SELF_UPDATE_ROOT"
git -C "$SELF_UPDATE_ROOT" add scripts/self-update.sh
git -C "$SELF_UPDATE_ROOT" commit -qm base
printf 'uncommitted optimization\n' >"$SELF_UPDATE_ROOT/optimization.rs"
expect_fail self_update_dirty env HOME="$TMP/self-update-home" \
  "$SELF_UPDATE_ROOT/scripts/self-update.sh"
grep -q 'refuses a dirty source tree' "$TMP/self_update_dirty.out"

echo "=== source-lock critical coverage ==="
for required in \
  .gitleaksignore \
  scripts/check-binary-tuple.sh \
  scripts/install-local.sh \
  scripts/onboarding-gate.sh \
  scripts/reconcile-evidence.sh \
  scripts/source-lock.sh \
  scripts/test-readiness-contract.sh \
  scripts/test-onboarding-gate.sh \
  scripts/verify-readiness.sh
do
  grep -Fq "\"$required\"" "$ROOT/scripts/source-lock.sh" || \
    fail "source-lock missing $required"
done

echo "=== readiness fixture ==="
FIX="$TMP/readiness"
FIX_HOME="$FIX/home"
mkdir -p \
  "$FIX/scripts" \
  "$FIX/artifacts/readiness" \
  "$FIX/agent/target/release" \
  "$FIX_HOME/.local/bin"
cp "$ROOT/scripts/verify-readiness.sh" "$FIX/scripts/verify-readiness.sh"
cp "$ROOT/scripts/reconcile-evidence.sh" "$FIX/scripts/reconcile-evidence.sh"
cp "$ROOT/scripts/check-binary-tuple.sh" "$FIX/scripts/check-binary-tuple.sh"
cp "$ROOT/scripts/source-lock.sh" "$FIX/scripts/source-lock.sh"
chmod +x "$FIX/scripts/"*.sh
printf '# exact fixture ignore\n' >"$FIX/.gitleaksignore"
python3 - "$FIX/LEGAL.md" <<'PY'
from pathlib import Path
import sys
Path(sys.argv[1]).write_text("legal fixture\n" + ("x" * 300) + "\n")
PY

for name in \
  assert-defaults.sh smoke-security.sh smoke-m2.sh parity-run.sh eval-coding.sh \
  smoke-verify.sh doctor-verticals.sh smoke-deepseek.sh smoke-r0-min.sh
do
  printf '#!/bin/sh\nexit 0\n' >"$FIX/scripts/$name"
  chmod +x "$FIX/scripts/$name"
done

make_layer_script() {
  local script="$1" file="$2" check_id="$3"
  python3 - "$FIX/scripts/$script" "$file" "$check_id" <<'PY'
from pathlib import Path
import sys
script, filename, check_id = sys.argv[1:4]
Path(script).write_text(f'''#!/bin/sh
set -eu
ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
python3 - "$ROOT/artifacts/readiness/{filename}" <<'PY2'
import json, sys
from pathlib import Path
doc = {{"schema_version": 1, "check_id": "{check_id}", "pass": True, "generated_at": "fixture"}}
if "{check_id}" == "L4_full":
    doc.update({{"scope": "full_contract_short", "scenarios": {{
        "429": {{"pass": True, "agent_request_count": 3, "effect_count": 1}},
        "500": {{"pass": True, "agent_request_count": 3, "effect_count": 1}},
        "disconnect": {{"pass": True, "agent_request_count": 3, "effect_count": 1}},
        "timeout": {{"pass": True, "agent_request_count": 1, "effect_count": 0}},
        "cancel": {{"pass": True, "agent_request_count": 1, "effect_count": 0}},
    }}}})
if "{check_id}" == "L5_full":
    doc.update({{
        "scope": "full_contract_soak", "resume_same_session": True,
        "compaction_persisted": True, "cache_visible": True,
        "update_event_id_count": 8, "update_event_ids_unique": True,
        "soak": {{"executed": True, "elapsed_seconds": 3600, "resume_turns": 2}},
    }})
Path(sys.argv[1]).write_text(json.dumps(doc, indent=2) + "\\n")
PY2
''')
PY
  chmod +x "$FIX/scripts/$script"
}

make_layer_script smoke-deepseek-agent.sh L1-tool-calls.json L1
make_layer_script smoke-deepseek-l2.sh L2-min-e2e.json L2
make_layer_script smoke-deepseek-l3.sh L3-multi-tool.json L3
make_layer_script smoke-deepseek-l4.sh L4-fault-cancel.json L4_full
make_layer_script smoke-deepseek-l5.sh L5-long-session.json L5_full

python3 - "$FIX/scripts/smoke-r0.sh" <<'PY'
from pathlib import Path
import sys
Path(sys.argv[1]).write_text(r'''#!/bin/sh
set -eu
ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/agent/target/release/lumen"
SHA="$(shasum -a 256 "$BIN" | awk '{print $1}')"
python3 - "$ROOT/artifacts/readiness" "$BIN" "$SHA" <<'PY2'
import json, sys
from pathlib import Path
art, binary, sha = sys.argv[1:4]
root = Path(art)
root.joinpath("R0-full.json").write_text(json.dumps({"schema_version": 1, "check_id": "R0_full", "pass": True, "scope": "full_contract", "binary": binary, "binary_sha256": sha}, indent=2) + "\n")
root.joinpath("R0-min.json").write_text(json.dumps({"schema_version": 1, "check_id": "R0_min", "pass": True}, indent=2) + "\n")
PY2
''')
PY
chmod +x "$FIX/scripts/smoke-r0.sh"

python3 - "$FIX/scripts/eval-coding-live.sh" <<'PY'
from pathlib import Path
import sys
Path(sys.argv[1]).write_text(r'''#!/bin/sh
set -eu
ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
: >"$ROOT/eval-called"
python3 - "$ROOT/artifacts/readiness/eval-live.json" "$HOME/.local/bin/lumen" <<'PY2'
import json, sys
from pathlib import Path
out, binary = sys.argv[1:3]
tasks = [{"task": f"task-{index:02d}", "result": "PASS", "agent_ec": 0} for index in range(20)]
doc = {"schema_version": 1, "check_id": "eval_live", "pass": True, "pass_count": 20, "fail_count": 0, "total": 20, "min_required": 18, "silent_corruption": 0, "tasks": tasks, "binary": binary, "generated_at": "fixture-live"}
Path(out).write_text(json.dumps(doc, indent=2) + "\n")
PY2
''')
PY
chmod +x "$FIX/scripts/eval-coding-live.sh"

python3 - "$FIX/scripts/generate-sbom.sh" <<'PY'
from pathlib import Path
import sys
Path(sys.argv[1]).write_text(r'''#!/bin/sh
set -eu
ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
python3 - "$ROOT" <<'PY2'
import hashlib, json, subprocess, sys
from pathlib import Path
root = Path(sys.argv[1])
head = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()
lock_sha = hashlib.sha256(root.joinpath("SOURCE_LOCK.json").read_bytes()).hexdigest()
doc = {
  "spdxVersion": "SPDX-2.3", "name": f"lumen-{head[:7]}",
  "packages": [{"name": "lumen", "externalRefs": [{"referenceType": "gitCommit", "referenceLocator": head}]}],
  "annotations": [{"comment": json.dumps({"monorepo_git_head": head, "file_sha256": {"SOURCE_LOCK.json": lock_sha}})}],
}
root.joinpath("SBOM.spdx.json").write_text(json.dumps(doc, indent=2) + "\n")
PY2
''')
PY
chmod +x "$FIX/scripts/generate-sbom.sh"

printf '#!/bin/sh\nexit 1\n' >"$FIX/scripts/productivity-gate.sh"
chmod +x "$FIX/scripts/productivity-gate.sh"
printf '#!/bin/sh\nexit 1\n' >"$FIX/scripts/onboarding-gate.sh"
chmod +x "$FIX/scripts/onboarding-gate.sh"

# source-lock is intentionally fail-closed on a missing critical file. Populate
# inert placeholders for critical paths that are irrelevant to this isolated
# verifier contract fixture; real scripts created above remain untouched.
python3 - "$FIX" "$ROOT/scripts/source-lock.sh" <<'PY'
from pathlib import Path
import re, sys

fixture = Path(sys.argv[1])
source = Path(sys.argv[2]).read_text()
match = re.search(r"paths = \[(.*?)\n\]", source, re.S)
assert match, "source-lock paths list not found"
for relative in re.findall(r'^\s*"([^"]+)",?\s*$', match.group(1), re.M):
    path = fixture / relative
    if path.exists():
        continue
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"readiness fixture placeholder: {relative}\n")
PY

init_git "$FIX"
git -C "$FIX" add .gitleaksignore LEGAL.md scripts
git -C "$FIX" commit -qm base
FIX_SHORT="$(git -C "$FIX" rev-parse --short=7 HEAD)"
printf '#!/bin/sh\necho "lumen fixture (%s)"\n' "$FIX_SHORT" \
  >"$FIX/agent/target/release/lumen"
chmod +x "$FIX/agent/target/release/lumen"
cp "$FIX/agent/target/release/lumen" "$FIX_HOME/.local/bin/lumen"

printf '#!/bin/sh\nexit 0\n' >"$FIX_HOME/.local/bin/gitleaks"
chmod +x "$FIX_HOME/.local/bin/gitleaks"

HOME="$FIX_HOME" "$FIX/scripts/source-lock.sh" >/dev/null

# A stale, valid-looking eval artifact must not be reused without EVAL_LIVE=1.
python3 - "$FIX/artifacts/readiness/eval-live.json" "$FIX_HOME/.local/bin/lumen" <<'PY'
import json, sys
from pathlib import Path
tasks = [{"task": f"old-{index:02d}", "result": "PASS", "agent_ec": 0} for index in range(20)]
Path(sys.argv[1]).write_text(json.dumps({"schema_version": 1, "check_id": "eval_live", "pass": True, "pass_count": 20, "fail_count": 0, "total": 20, "min_required": 18, "silent_corruption": 0, "tasks": tasks, "binary": sys.argv[2], "generated_at": "stale"}, indent=2) + "\n")
PY

expect_fail verify_skip_is_not_green env HOME="$FIX_HOME" DEEPSEEK_API_KEY=fake \
  "$FIX/scripts/verify-readiness.sh"
cp "$TMP/verify_skip_is_not_green.out" "$TMP/verify-skip.out"
[[ ! -e "$FIX/eval-called" ]] || fail "old eval path invoked live script"
python3 - "$FIX/artifacts/readiness/status.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
check = next(item for item in d["checks"] if item["id"] == "eval_live")
assert check["result"] == "SKIP", check
assert d["engineering_complete"] is False
PY

echo "=== binary tuple mismatch ==="
printf '\n# changed bytes\n' >>"$FIX_HOME/.local/bin/lumen"
expect_fail binary_mismatch env HOME="$FIX_HOME" "$FIX/scripts/check-binary-tuple.sh"
cp "$FIX/agent/target/release/lumen" "$FIX_HOME/.local/bin/lumen"
chmod +x "$FIX_HOME/.local/bin/lumen"

echo "=== explicit live fixture + semantic reconcile ==="
HOME="$FIX_HOME" DEEPSEEK_API_KEY=fake EVAL_LIVE=1 \
  "$FIX/scripts/verify-readiness.sh" >"$TMP/verify-live.out"
[[ -e "$FIX/eval-called" ]] || fail "EVAL_LIVE=1 did not invoke live script"
python3 - "$FIX/artifacts/readiness/status.json" "$FIX/artifacts/readiness/reconcile.json" <<'PY'
import json, sys
status = json.load(open(sys.argv[1]))
rec = json.load(open(sys.argv[2]))
assert rec["pass"] is True, rec["blockers"]
assert rec["source_lock_head_match"] is True
assert rec["binary_tuple_match"] is True
assert rec["artifact_semantics_ok"] is True
assert rec["status_semantics_ok"] is True
assert rec["sbom_semantics_ok"] is True
assert status["engineering_complete"] is True, status["blockers"]
assert status["ready"] is False
assert status["blockers"] == [
    "M5_10_min_stranger:human_gate missing_or_invalid",
    "M6_15_day_self_use:human_gate count_lt_15",
]
PY

# Materially identical reconcile keeps its old timestamp.
python3 - "$FIX/artifacts/readiness/reconcile.json" <<'PY'
import json, sys
from pathlib import Path
p = Path(sys.argv[1]); d = json.loads(p.read_text()); d["generated_at"] = "2000-01-01T00:00:00Z"
p.write_text(json.dumps(d, indent=2) + "\n")
PY
HOME="$FIX_HOME" "$FIX/scripts/reconcile-evidence.sh" >/dev/null
python3 - "$FIX/artifacts/readiness/reconcile.json" <<'PY'
import json, sys
assert json.load(open(sys.argv[1]))["generated_at"] == "2000-01-01T00:00:00Z"
PY

# A failed current-run status row invalidates reconciliation.
cp "$FIX/artifacts/readiness/status.json" "$TMP/status.good"
python3 - "$FIX/artifacts/readiness/status.json" <<'PY'
import json, sys
from pathlib import Path
p = Path(sys.argv[1]); d = json.loads(p.read_text())
row = next(item for item in d["checks"] if item["id"] == "L3_multi_tool")
row.update({"pass": False, "result": "FAIL"})
p.write_text(json.dumps(d, indent=2) + "\n")
PY
expect_fail reconcile_status_semantics env HOME="$FIX_HOME" "$FIX/scripts/reconcile-evidence.sh"
grep -q 'status_check_not_pass:L3_multi_tool' "$TMP/reconcile_status_semantics.out"
cp "$TMP/status.good" "$FIX/artifacts/readiness/status.json"

# A 19-task eval cannot satisfy the 20-task contract even with pass=true.
cp "$FIX/artifacts/readiness/eval-live.json" "$TMP/eval.good"
python3 - "$FIX/artifacts/readiness/eval-live.json" <<'PY'
import json, sys
from pathlib import Path
p = Path(sys.argv[1]); d = json.loads(p.read_text()); d["total"] = 19
p.write_text(json.dumps(d, indent=2) + "\n")
PY
expect_fail reconcile_eval_semantics env HOME="$FIX_HOME" "$FIX/scripts/reconcile-evidence.sh"
grep -q 'eval_live_semantics_invalid' "$TMP/reconcile_eval_semantics.out"
cp "$TMP/eval.good" "$FIX/artifacts/readiness/eval-live.json"

# HEAD is material: a new commit must update reconcile and fail the old lock.
printf 'head drift\n' >"$FIX/head-drift.txt"
git -C "$FIX" add head-drift.txt
git -C "$FIX" commit -qm drift
NEW_HEAD="$(git -C "$FIX" rev-parse HEAD)"
expect_fail reconcile_head_drift env HOME="$FIX_HOME" "$FIX/scripts/reconcile-evidence.sh"
python3 - "$FIX/artifacts/readiness/reconcile.json" "$NEW_HEAD" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
assert d["monorepo_git_head"] == sys.argv[2]
assert d["source_lock_head_match"] is False
PY

echo "OK: readiness contract deterministic tests passed"
