#!/usr/bin/env bash
# FINAL-2.0 L4 full short contract.
#
# Deterministic localhost fault injection drives the shipped Lumen binary
# through 429, 5xx, mid-stream disconnect, idle timeout, and cancellation.
# Retry-success cases must execute exactly one observable side effect. The
# shipped sampler intentionally treats IdleTimeout as terminal; that case must
# fail promptly with zero side effects instead of being mislabeled as retried.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
PROOF="${LUMEN_EVIDENCE_DIR:-$ROOT/artifacts/readiness}"
CASES="${LUMEN_L4_CASE_DIR-$PROOF/L4-full-cases}"
FIXTURE="$ROOT/scripts/fixtures/final20_openai_fault_server.py"
BIN="${LUMEN_BINARY:-$ROOT/agent/target/release/lumen}"
CASE_TIMEOUT_SECONDS="${LUMEN_L4_CASE_TIMEOUT_SECONDS:-30}"
IDLE_TIMEOUT_SECONDS="${LUMEN_L4_IDLE_TIMEOUT_SECONDS:-1}"

for value in "$CASE_TIMEOUT_SECONDS" "$IDLE_TIMEOUT_SECONDS"; do
  [[ "$value" =~ ^[1-9][0-9]*$ ]] || {
    echo "FAIL: L4 timeout values must be positive integers" >&2
    exit 2
  }
done
[[ -f "$FIXTURE" ]] || { echo "FAIL: missing fixture $FIXTURE" >&2; exit 1; }
mkdir -p "$PROOF"

resolve_safe_case_dir() {
  "$PYTHON_BIN" - "$1" "$PROOF" "$ROOT" "$HOME" "L4" <<'PY'
import os, sys
from pathlib import Path

raw, proof_raw, root_raw, home_raw, prefix = sys.argv[1:]
if not raw:
    raise SystemExit("FAIL: L4 case directory must not be empty")
case = Path(os.path.realpath(os.path.abspath(raw)))
proof = Path(os.path.realpath(os.path.abspath(proof_raw)))
forbidden = {
    Path("/"),
    Path(os.path.realpath(os.path.abspath(root_raw))),
    Path(os.path.realpath(os.path.abspath(home_raw))),
}
if proof in forbidden:
    raise SystemExit(f"FAIL: unsafe L4 evidence root: {proof}")
if case in forbidden:
    raise SystemExit(f"FAIL: unsafe L4 case directory: {case}")
try:
    relative = case.relative_to(proof)
except ValueError:
    raise SystemExit(f"FAIL: L4 case directory escapes evidence root {proof}: {case}")
if not relative.parts or not relative.parts[0].startswith(prefix + "-"):
    raise SystemExit(
        f"FAIL: L4 case directory must be under a dedicated {prefix}-* subtree of {proof}: {case}"
    )
print(case)
PY
}

CASES="$(resolve_safe_case_dir "$CASES")"
echo "=== L4 case_dir=$CASES ==="
rm -rf -- "$CASES"
mkdir -p "$CASES"

if [[ ! -x "$BIN" ]]; then
  echo "=== build release Lumen binary ==="
  (cd "$ROOT/agent" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo build -p xai-grok-pager-bin --release)
fi
test -x "$BIN"

PIDS=()
cleanup() {
  local pid
  if (( ${#PIDS[@]} )); then
    for pid in "${PIDS[@]}"; do
      kill -TERM "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    done
  fi
}
trap cleanup EXIT

wait_for_file() {
  local path="$1" attempts="${2:-100}"
  local i
  for ((i = 0; i < attempts; i++)); do
    [[ -s "$path" ]] && return 0
    sleep 0.05
  done
  echo "FAIL: timed out waiting for $path" >&2
  return 1
}

wait_for_pattern() {
  local path="$1" pattern="$2" attempts="${3:-200}"
  local i
  for ((i = 0; i < attempts; i++)); do
    [[ -f "$path" ]] && grep -Fq "$pattern" "$path" && return 0
    sleep 0.05
  done
  echo "FAIL: timed out waiting for '$pattern' in $path" >&2
  return 1
}

stop_fixture() {
  local pid="$1"
  local active
  local -a remaining=()
  kill -TERM "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  for active in "${PIDS[@]}"; do
    [[ "$active" == "$pid" ]] || remaining+=("$active")
  done
  if (( ${#remaining[@]} )); then
    PIDS=("${remaining[@]}")
  else
    PIDS=()
  fi
}

start_fixture() {
  local scenario="$1" case_dir="$2" stall="$3"
  "$PYTHON_BIN" "$FIXTURE" \
    --scenario "$scenario" \
    --state-dir "$case_dir/server" \
    --port-file "$case_dir/port" \
    --marker-file "$case_dir/effect.log" \
    --stall-seconds "$stall" \
    >"$case_dir/server.stdout" 2>"$case_dir/server.stderr" &
  FIXTURE_PID=$!
  PIDS+=("$FIXTURE_PID")
  wait_for_file "$case_dir/port"
  FIXTURE_PORT="$(tr -d '[:space:]' <"$case_dir/port")"
  [[ "$FIXTURE_PORT" =~ ^[0-9]+$ ]] || {
    echo "FAIL: fixture returned invalid port" >&2
    return 1
  }
}

write_config() {
  local case_dir="$1" port="$2"
  mkdir -p "$case_dir/home" "$case_dir/grok-home"
  cat >"$case_dir/grok-home/config.toml" <<CFG
[models]
default = "l4-fixture"
max_retries = 2
inference_idle_timeout_secs = $IDLE_TIMEOUT_SECONDS

[model.l4-fixture]
model = "lumen-final20-fixture"
name = "FINAL-2.0 L4 localhost fixture"
base_url = "http://127.0.0.1:$port/v1"
api_backend = "chat_completions"
env_key = "LUMEN_FIXTURE_API_KEY"
context_window = 32768
max_completion_tokens = 1024
max_retries = 2
inference_idle_timeout_secs = $IDLE_TIMEOUT_SECONDS
agent_type = "grok-build-plan"

[cli]
auto_update = false
CFG
}

spawn_lumen() {
  local case_dir="$1"
  (
    export HOME="$case_dir/home"
    export GROK_HOME="$case_dir/grok-home"
    export LUMEN_HOME="$case_dir/grok-home"
    export LUMEN_FIXTURE_API_KEY="fixture-not-a-secret"
    export GROK_DISABLE_AUTOUPDATER=1
    export GROK_TELEMETRY_ENABLED=false
    export GROK_FEEDBACK_ENABLED=false
    export GROK_TRACE_UPLOAD=false
    export GROK_INSTRUMENTATION=disabled
    unset DEEPSEEK_API_KEY OPENAI_API_KEY ANTHROPIC_API_KEY ANTHROPIC_AUTH_TOKEN
    unset XAI_API_KEY GROK_CODE_XAI_API_KEY GROK_API_KEY GROK_AUTH GROK_AUTH_PATH
    exec "$BIN" \
      --model l4-fixture \
      --single "Use the terminal tool exactly once to create the requested L4 marker, then finish." \
      --output-format plain \
      --always-approve \
      --max-turns 6 \
      --disable-web-search \
      --no-subagents \
      --no-memory \
      --no-plan \
      --system-prompt-override "You are a deterministic L4 contract client. Execute fixture tool calls and do not ask questions." \
      --debug-file "$case_dir/debug.log"
  ) >"$case_dir/out.log" 2>"$case_dir/err.log" &
  LUMEN_PID=$!
}

wait_lumen() {
  local pid="$1" timeout_seconds="$2"
  local ticks=$((timeout_seconds * 10)) i
  for ((i = 0; i < ticks; i++)); do
    if ! kill -0 "$pid" 2>/dev/null; then
      set +e
      wait "$pid"
      LUMEN_EC=$?
      set -e
      return 0
    fi
    sleep 0.1
  done
  kill -TERM "$pid" 2>/dev/null || true
  sleep 0.5
  kill -KILL "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  LUMEN_EC=124
  return 0
}

assert_retry_case() {
  local scenario="$1" expected_fault="$2"
  local case_dir="$CASES/$scenario"
  mkdir -p "$case_dir"
  echo "=== L4 $scenario: transient fault -> retry -> one effect ==="
  start_fixture "$scenario" "$case_dir" 60
  local server_pid="$FIXTURE_PID" port="$FIXTURE_PORT"
  write_config "$case_dir" "$port"
  spawn_lumen "$case_dir"
  wait_lumen "$LUMEN_PID" "$CASE_TIMEOUT_SECONDS"
  stop_fixture "$server_pid"

  [[ "$LUMEN_EC" -eq 0 ]] || {
    echo "FAIL: $scenario Lumen exit=$LUMEN_EC" >&2
    tail -80 "$case_dir/err.log" "$case_dir/out.log" >&2 || true
    return 1
  }
  [[ -f "$case_dir/effect.log" ]] || { echo "FAIL: $scenario effect missing" >&2; return 1; }
  [[ "$(wc -l <"$case_dir/effect.log" | tr -d ' ')" == "1" ]] || {
    echo "FAIL: $scenario side effect was not exactly once" >&2
    return 1
  }
  [[ "$(cat "$case_dir/effect.log")" == "effect" ]] || {
    echo "FAIL: $scenario side effect content mismatch" >&2
    return 1
  }
  grep -Fq "\"kind\":\"$expected_fault\"" "$case_dir/server/events.jsonl"
  grep -Fq '"kind":"tool_call"' "$case_dir/server/events.jsonl"
  grep -Fq '"kind":"final"' "$case_dir/server/events.jsonl"
  "$PYTHON_BIN" - "$case_dir/server/events.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8")]
agent = [row for row in rows if row.get("event") == "request" and row.get("agent")]
assert len(agent) == 3, agent
assert [row["attempt"] for row in agent] == [1, 2, 3], agent
PY
}

assert_timeout_case() {
  local case_dir="$CASES/timeout"
  mkdir -p "$case_dir"
  echo "=== L4 timeout: terminal idle timeout, zero effect ==="
  start_fixture timeout "$case_dir" 10
  local server_pid="$FIXTURE_PID" port="$FIXTURE_PORT"
  write_config "$case_dir" "$port"
  spawn_lumen "$case_dir"
  wait_lumen "$LUMEN_PID" "$CASE_TIMEOUT_SECONDS"
  stop_fixture "$server_pid"

  [[ "$LUMEN_EC" -ne 0 && "$LUMEN_EC" -ne 124 ]] || {
    echo "FAIL: timeout must terminate non-zero before harness deadline (exit=$LUMEN_EC)" >&2
    return 1
  }
  [[ ! -e "$case_dir/effect.log" ]] || { echo "FAIL: timeout produced a side effect" >&2; return 1; }
  grep -Eiq 'idle.?timeout|timed out|timeout' "$case_dir/debug.log" "$case_dir/err.log"
  "$PYTHON_BIN" - "$case_dir/server/events.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8")]
agent = [row for row in rows if row.get("event") == "request" and row.get("agent")]
responses = [row.get("kind") for row in rows if row.get("event") == "response"]
assert len(agent) == 1, agent
assert responses.count("fault_timeout") == 1, responses
assert "tool_call" not in responses, responses
PY
}

assert_cancel_case() {
  local case_dir="$CASES/cancel"
  mkdir -p "$case_dir"
  echo "=== L4 cancel: SIGINT stops a live stalled binary, zero effect ==="
  start_fixture cancel "$case_dir" 60
  local server_pid="$FIXTURE_PID" port="$FIXTURE_PORT"
  write_config "$case_dir" "$port"
  spawn_lumen "$case_dir"
  local lumen_pid="$LUMEN_PID"
  wait_for_pattern "$case_dir/server/events.jsonl" '"kind":"fault_cancel"'
  local started ended
  started="$(date +%s)"
  kill -INT "$lumen_pid"
  wait_lumen "$lumen_pid" 10
  ended="$(date +%s)"
  stop_fixture "$server_pid"

  [[ "$LUMEN_EC" -ne 124 ]] || { echo "FAIL: cancel did not stop Lumen promptly" >&2; return 1; }
  [[ $((ended - started)) -le 10 ]] || { echo "FAIL: cancel exceeded 10s" >&2; return 1; }
  [[ ! -e "$case_dir/effect.log" ]] || { echo "FAIL: cancel produced a side effect" >&2; return 1; }
  "$PYTHON_BIN" - "$case_dir/server/events.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8")]
responses = [row.get("kind") for row in rows if row.get("event") == "response"]
assert responses.count("fault_cancel") == 1, responses
assert "tool_call" not in responses, responses
PY
}

assert_retry_case 429 fault_429
assert_retry_case 500 fault_500
assert_retry_case disconnect fault_disconnect
assert_timeout_case
assert_cancel_case

echo "=== L4 process-tree cancellation contract ==="
(
  cd "$ROOT/agent"
  cargo test -p xai-tty-utils --lib kill_all -- --nocapture
)

"$PYTHON_BIN" - "$PROOF/L4-fault-cancel.json" "$BIN" "$CASES" <<'PY'
import hashlib, json, sys
from datetime import datetime, timezone
from pathlib import Path

out, binary, cases = map(Path, sys.argv[1:])
scenario_rows = {}
for name in ("429", "500", "disconnect", "timeout", "cancel"):
    rows = [json.loads(line) for line in (cases / name / "server" / "events.jsonl").read_text().splitlines()]
    requests = [row for row in rows if row.get("event") == "request" and row.get("agent")]
    responses = [row.get("kind") for row in rows if row.get("event") == "response"]
    effect = cases / name / "effect.log"
    scenario_rows[name] = {
        "pass": True,
        "agent_request_count": len(requests),
        "response_kinds": responses,
        "effect_count": len(effect.read_text().splitlines()) if effect.exists() else 0,
    }

artifact = {
    "schema_version": 1,
    "check_id": "L4_full",
    "pass": True,
    "scope": "full_contract_short",
    "binary": str(binary),
    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "scenarios": scenario_rows,
    "retry_policy": "429, 5xx, and interrupted SSE retry; IdleTimeout is terminal by shipped policy",
    "side_effect_invariant": "retry-success cases execute exactly one effect; timeout/cancel execute zero",
    "process_tree_suite": "cargo test -p xai-tty-utils --lib kill_all",
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
}
out.write_text(json.dumps(artifact, indent=2) + "\n")
print(json.dumps({"pass": True, "check_id": "L4_full", "artifact": str(out)}))
PY

echo "OK: L4 full short contract passed (429/5xx/disconnect retry, timeout, cancel, idempotency)"
