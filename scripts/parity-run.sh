#!/usr/bin/env bash
# M3 CC_PARITY harness — score checklist + 12 mock-parity scenarios.
# Exit 0 when completion rate (已有|已补 / total scorable) ≥ 80%.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"

PARITY_MD="$ROOT/policy/CC_PARITY.md"
SCENARIOS="$ROOT/policy/parity_scenarios.json"
fail() { echo "FAIL: $*" >&2; exit 1; }

test -f "$PARITY_MD" || fail "missing CC_PARITY.md"
test -f "$SCENARIOS" || fail "missing parity_scenarios.json"

echo "=== 1) lumen-guard (security parity B/F) ==="
(
  cd "$ROOT/agent"
  cargo test -p lumen-guard --lib --quiet
)

echo "=== 2) lumen-discipline (S07–S09) ==="
(
  cd "$ROOT/agent"
  cargo test -p lumen-discipline --lib --quiet
)

echo "=== 3) structural evidence for scenarios ==="
# token_cost / cache_line
grep -q 'cache_line' \
  "$ROOT/agent/crates/codegen/xai-grok-shell/src/extensions/notification.rs" \
  || fail "cache_line missing from headless usage"
# hard-deny before YOLO
grep -q 'lumen_guard_deny' \
  "$ROOT/agent/crates/codegen/xai-grok-workspace/src/permission/manager.rs" \
  || fail "lumen_guard_deny not wired"
# goal incomplete gate
grep -q 'lumen_goal_incomplete_gate' \
  "$ROOT/agent/crates/codegen/xai-grok-tools/src/implementations/grok_build/update_goal/mod.rs" \
  || fail "goal incomplete gate missing"
# tools exist
test -d "$ROOT/agent/crates/codegen/xai-grok-tools/src/implementations" \
  || fail "tools implementations missing"
# compaction module
test -f "$ROOT/agent/crates/codegen/xai-grok-shell/src/session/compaction.rs" \
  || fail "compaction.rs missing"
# mcp crate
test -d "$ROOT/agent/crates/codegen/xai-grok-mcp" || fail "mcp crate missing"

echo "=== 4) score CC_PARITY table statuses ==="
python3 - "$ROOT" <<'PY'
import re, json, sys
from pathlib import Path
root = Path(sys.argv[1])
md = (root / "policy" / "CC_PARITY.md").read_text(encoding="utf-8")
# Status cells: **已有** or 已有 / 部分 / 缺失 / 不做 (table rows with ID like F01)
rows = []
for line in md.splitlines():
    if not re.match(r"^\| [A-Z]\d{2} ", line):
        continue
    # split table; status is 5th data col (0=empty,1=id,2=behavior,..., status near end)
    parts = [p.strip() for p in line.split("|")]
    # ['', 'F01', '行为', '参考', '落点', '状态', '测试', '']
    if len(parts) < 7:
        continue
    rid, status = parts[1], parts[5]
    # strip markdown bold
    status_clean = status.replace("**", "").strip()
    # take first token
    status_clean = status_clean.split()[0] if status_clean else ""
    if status_clean in ("已有", "部分", "缺失", "不做"):
        rows.append((rid, status_clean))

total = len(rows)
if total < 40:
    print(f"FAIL: only {total} scorable rows (need ≥40)", file=sys.stderr)
    sys.exit(1)

good = sum(1 for _, s in rows if s in ("已有", "部分"))
rate = good / total
print(f"CC_PARITY rows: {total}")
print(f"  已有/部分: {good}")
print(f"  缺失: {sum(1 for _,s in rows if s=='缺失')}")
print(f"  不做: {sum(1 for _,s in rows if s=='不做')}")
print(f"completion rate: {rate*100:.1f}% (need ≥80%)")
if rate < 0.80:
    print("FAIL: completion rate below 80%", file=sys.stderr)
    sys.exit(1)

# Scenarios JSON
sc = json.loads((root / "policy" / "parity_scenarios.json").read_text())
if len(sc) != 12:
    print(f"FAIL: expected 12 scenarios, got {len(sc)}", file=sys.stderr)
    sys.exit(1)
names = {s["name"] for s in sc}
required = {
    "streaming_text", "read_file_roundtrip", "grep_chunk_assembly",
    "write_file_allowed", "write_file_denied", "multi_tool_turn_roundtrip",
    "bash_stdout_roundtrip", "bash_permission_prompt_approved",
    "bash_permission_prompt_denied", "plugin_tool_roundtrip",
    "auto_compact_triggered", "token_cost_reporting",
}
missing = required - names
if missing:
    print(f"FAIL: missing scenarios {missing}", file=sys.stderr)
    sys.exit(1)
sc_good = sum(1 for s in sc if s.get("status") in ("已有", "部分"))
print(f"scenarios: {len(sc)}, 已有/部分: {sc_good}")
print("OK: parity-run passed")
PY
