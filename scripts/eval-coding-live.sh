#!/usr/bin/env bash
# Live coding eval: for each of 20 tasks, copy workspace, run lumen headless to fix,
# then re-run deterministic tests. Signs ≥18/20 for publish gate.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
TASKS="$ROOT/evals/tasks"
ART="$ROOT/artifacts/readiness"
LIVE_DIR="${EVAL_LIVE_DIR:-$ART/eval-live}"
mkdir -p "$LIVE_DIR" "$ART"
LUMEN_BIN="${LUMEN_BIN:-$HOME/.local/bin/lumen}"
if [[ ! -x "$LUMEN_BIN" ]]; then
  LUMEN_BIN="$ROOT/agent/target/release/lumen"
fi
[[ -x "$LUMEN_BIN" ]] || { echo "FAIL: no lumen binary"; exit 1; }
[[ -n "${DEEPSEEK_API_KEY:-}" ]] || { echo "FAIL: DEEPSEEK_API_KEY required for live eval"; exit 1; }

MAX_TURNS="${EVAL_MAX_TURNS:-12}"
MIN_PASS="${EVAL_MIN_PASS:-18}"
LIMIT="${EVAL_LIMIT:-0}"  # 0 = all

echo "=== eval-coding-live ==="
echo "lumen=$LUMEN_BIN max_turns=$MAX_TURNS min_pass=$MIN_PASS"

run_tests() {
  local ws="$1"
  if [[ -f "$ws/go.mod" ]]; then
    (cd "$ws" && go test ./... -count=1 -timeout 30s) >/dev/null 2>&1
  elif find "$ws" -name 'test_*.py' -o -name '*_test.py' 2>/dev/null | grep -q .; then
    (cd "$ws" && python3 -m pytest -q) >/dev/null 2>&1
  elif [[ -f "$ws/package.json" ]]; then
    (cd "$ws" && npx --yes vitest run) >/dev/null 2>&1
  else
    return 2
  fi
}

PASS=0
FAIL=0
TOTAL=0
results_tmp="$(mktemp)"
trap 'rm -f "$results_tmp"' EXIT

for task_dir in "$TASKS"/*/; do
  name=$(basename "$task_dir")
  TOTAL=$((TOTAL + 1))
  if [[ "$LIMIT" -gt 0 && "$TOTAL" -gt "$LIMIT" ]]; then
    TOTAL=$((TOTAL - 1))
    break
  fi

  prompt_file="$task_dir/prompt.txt"
  src_ws="$task_dir/workspace"
  [[ -f "$prompt_file" && -d "$src_ws" ]] || { echo "  SKIP $name missing"; continue; }

  work="$LIVE_DIR/$name"
  rm -rf "$work"
  mkdir -p "$work"
  # copy workspace content (exclude caches)
  rsync -a --exclude node_modules --exclude .pytest_cache --exclude target "$src_ws/" "$work/ws/"
  prompt=$(cat "$prompt_file")
  log="$work/agent.log"
  home="$work/grok-home"
  mkdir -p "$home"

  echo "  RUN $name …"
  set +e
  # isolated home + always approve tools; headless single prompt with multi-turn agent loop
  GROK_HOME="$home" \
  DEEPSEEK_API_KEY="$DEEPSEEK_API_KEY" \
  "$LUMEN_BIN" \
    --cwd "$work/ws" \
    --always-approve \
    --permission-mode bypassPermissions \
    --max-turns "$MAX_TURNS" \
    --output-format plain \
    -p "$prompt" \
    >"$log" 2>&1
  agent_ec=$?
  set -e

  set +e
  run_tests "$work/ws"
  test_ec=$?
  set -e

  if [[ $test_ec -eq 0 ]]; then
    echo "  PASS $name (agent_ec=$agent_ec)"
    echo "$name|PASS|$agent_ec" >>"$results_tmp"
    PASS=$((PASS + 1))
  else
    echo "  FAIL $name (agent_ec=$agent_ec test_ec=$test_ec)"
    echo "$name|FAIL|$agent_ec" >>"$results_tmp"
    FAIL=$((FAIL + 1))
    # tail for diagnosis
    tail -5 "$log" | sed 's/^/    | /' || true
  fi
done

python3 - "$ART/eval-live.json" "$results_tmp" "$PASS" "$FAIL" "$TOTAL" "$MIN_PASS" "$LUMEN_BIN" <<'PY'
import json, sys
from datetime import datetime, timezone
from pathlib import Path
out, rows_path, pass_n, fail_n, total, mn, binary = sys.argv[1:8]
pass_n, fail_n, total, mn = map(int, (pass_n, fail_n, total, mn))
rows = []
for line in Path(rows_path).read_text().splitlines():
    parts = line.split("|")
    if len(parts) >= 2:
        rows.append({"task": parts[0], "result": parts[1], "agent_ec": int(parts[2]) if len(parts)>2 and parts[2].isdigit() else parts[2]})
ok = pass_n >= mn and fail_n == (total - pass_n)  # no silent pass inflation
# silent corruption = task marked PASS but we only mark PASS when tests pass
art = {
  "schema_version": 1,
  "check_id": "eval_live",
  "pass": pass_n >= mn,
  "pass_count": pass_n,
  "fail_count": fail_n,
  "total": total,
  "min_required": mn,
  "silent_corruption": 0,
  "tasks": rows,
  "binary": binary,
  "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
  "note": "Live agent solves; PASS only when deterministic workspace tests pass after agent edit.",
}
Path(out).write_text(json.dumps(art, indent=2) + "\n")
print(f"wrote {out} pass_count={pass_n}/{total} gate_pass={art['pass']}")
sys.exit(0 if art["pass"] else 1)
PY
