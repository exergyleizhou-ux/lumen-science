#!/usr/bin/env bash
# FINAL-2.0 L5 full contract.
#
# Default mode is a CI-safe deterministic short run. Explicit soak mode keeps
# resuming the same real Lumen session for at least one hour by default:
#   LUMEN_L5_MODE=soak ./scripts/smoke-deepseek-l5.sh
# The short run never claims that the one-hour soak was executed.
# Default artifacts are isolated by mode:
#   short -> artifacts/readiness/L5-long-session-short.json + L5-full-case-short/
#   soak  -> artifacts/readiness/L5-long-session.json       + L5-full-case-soak/
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
PROOF="${LUMEN_EVIDENCE_DIR:-$ROOT/artifacts/readiness}"
FIXTURE="$ROOT/scripts/fixtures/final20_openai_fault_server.py"
BIN="${LUMEN_BINARY:-$ROOT/agent/target/release/lumen}"
MODE="${LUMEN_L5_MODE:-short}"
CASE_TIMEOUT_SECONDS="${LUMEN_L5_CASE_TIMEOUT_SECONDS:-45}"
SOAK_SECONDS="${LUMEN_L5_SOAK_SECONDS:-3600}"
SOAK_INTERVAL_SECONDS="${LUMEN_L5_SOAK_INTERVAL_SECONDS:-30}"
MIN_SOAK_SECONDS=3600
CANONICAL_SOAK_ARTIFACT="${LUMEN_L5_CANONICAL_SOAK_ARTIFACT:-$PROOF/L5-long-session.json}"
CANONICAL_SOAK_CASE_DIR="${LUMEN_L5_CANONICAL_SOAK_CASE_DIR:-$PROOF/L5-full-case-soak}"

[[ "$MODE" == "short" || "$MODE" == "soak" ]] || {
  echo "FAIL: LUMEN_L5_MODE must be short or soak" >&2
  exit 2
}
for value in "$CASE_TIMEOUT_SECONDS" "$SOAK_SECONDS" "$SOAK_INTERVAL_SECONDS"; do
  [[ "$value" =~ ^[1-9][0-9]*$ ]] || {
    echo "FAIL: L5 duration values must be positive integers" >&2
    exit 2
  }
done
if [[ "$MODE" == "soak" && "$SOAK_SECONDS" -lt "$MIN_SOAK_SECONDS" ]]; then
  echo "FAIL: soak mode requires LUMEN_L5_SOAK_SECONDS >= $MIN_SOAK_SECONDS" >&2
  echo "Use LUMEN_L5_MODE=short for CI; do not label a shorter run as the one-hour soak." >&2
  exit 2
fi

if [[ "$MODE" == "short" ]]; then
  ARTIFACT="${LUMEN_L5_ARTIFACT:-$PROOF/L5-long-session-short.json}"
  CASE_DIR="${LUMEN_L5_CASE_DIR-$PROOF/L5-full-case-short}"
else
  ARTIFACT="${LUMEN_L5_ARTIFACT:-$CANONICAL_SOAK_ARTIFACT}"
  CASE_DIR="${LUMEN_L5_CASE_DIR-$CANONICAL_SOAK_CASE_DIR}"
fi

path_identity() {
  "$PYTHON_BIN" - "$1" <<'PY'
import os, sys
print(os.path.realpath(os.path.abspath(sys.argv[1])))
PY
}

resolve_safe_case_dir() {
  "$PYTHON_BIN" - "$1" "$PROOF" "$ROOT" "$HOME" "L5" <<'PY'
import os, sys
from pathlib import Path

raw, proof_raw, root_raw, home_raw, prefix = sys.argv[1:]
if not raw:
    raise SystemExit("FAIL: L5 case directory must not be empty")
case = Path(os.path.realpath(os.path.abspath(raw)))
proof = Path(os.path.realpath(os.path.abspath(proof_raw)))
forbidden = {
    Path("/"),
    Path(os.path.realpath(os.path.abspath(root_raw))),
    Path(os.path.realpath(os.path.abspath(home_raw))),
}
if proof in forbidden:
    raise SystemExit(f"FAIL: unsafe L5 evidence root: {proof}")
if case in forbidden:
    raise SystemExit(f"FAIL: unsafe L5 case directory: {case}")
try:
    relative = case.relative_to(proof)
except ValueError:
    raise SystemExit(f"FAIL: L5 case directory escapes evidence root {proof}: {case}")
if not relative.parts or not relative.parts[0].startswith(prefix + "-"):
    raise SystemExit(
        f"FAIL: L5 case directory must be under a dedicated {prefix}-* subtree of {proof}: {case}"
    )
print(case)
PY
}

CASE_DIR="$(resolve_safe_case_dir "$CASE_DIR")"

if [[ "$MODE" == "short" ]]; then
  [[ "$(path_identity "$ARTIFACT")" != "$(path_identity "$CANONICAL_SOAK_ARTIFACT")" ]] || {
    echo "FAIL: short mode must not overwrite canonical soak artifact $CANONICAL_SOAK_ARTIFACT" >&2
    exit 2
  }
  [[ "$(path_identity "$CASE_DIR")" != "$(path_identity "$CANONICAL_SOAK_CASE_DIR")" ]] || {
    echo "FAIL: short mode must not overwrite canonical soak case directory $CANONICAL_SOAK_CASE_DIR" >&2
    exit 2
  }
fi

[[ -f "$FIXTURE" ]] || { echo "FAIL: missing fixture $FIXTURE" >&2; exit 1; }
echo "=== L5 mode=$MODE artifact=$ARTIFACT case_dir=$CASE_DIR ==="
mkdir -p "$PROOF" "$(dirname "$ARTIFACT")"
rm -rf -- "$CASE_DIR"
mkdir -p "$CASE_DIR/workspace" "$CASE_DIR/home" "$CASE_DIR/grok-home" "$CASE_DIR/server"

if [[ ! -x "$BIN" ]]; then
  echo "=== build release Lumen binary ==="
  (cd "$ROOT/agent" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo build -p xai-grok-pager-bin --release)
fi
test -x "$BIN"

SESSION_ID="$($PYTHON_BIN - <<'PY'
import uuid
print(uuid.uuid4())
PY
)"
SESSION_TOKEN="L5_SESSION_${SESSION_ID}"
MARKER_FILE="$CASE_DIR/workspace/session_token.txt"
PORT_FILE="$CASE_DIR/port"
SERVER_EVENTS="$CASE_DIR/server/events.jsonl"

SERVER_PID=""
cleanup() {
  if [[ -n "$SERVER_PID" ]]; then
    kill -TERM "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
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

"$PYTHON_BIN" "$FIXTURE" \
  --scenario l5 \
  --state-dir "$CASE_DIR/server" \
  --port-file "$PORT_FILE" \
  --marker-file "$MARKER_FILE" \
  --session-token "$SESSION_TOKEN" \
  --stall-seconds 60 \
  --compaction-prompt-tokens 30000 \
  >"$CASE_DIR/server.stdout" 2>"$CASE_DIR/server.stderr" &
SERVER_PID=$!
wait_for_file "$PORT_FILE"
PORT="$(tr -d '[:space:]' <"$PORT_FILE")"
[[ "$PORT" =~ ^[0-9]+$ ]] || { echo "FAIL: invalid L5 fixture port" >&2; exit 1; }

cat >"$CASE_DIR/grok-home/config.toml" <<CFG
[models]
default = "l5-fixture"
max_retries = 2
inference_idle_timeout_secs = 5

[model.l5-fixture]
model = "lumen-final20-fixture"
name = "FINAL-2.0 L5 localhost fixture"
base_url = "http://127.0.0.1:$PORT/v1"
api_backend = "chat_completions"
env_key = "LUMEN_FIXTURE_API_KEY"
context_window = 32768
max_completion_tokens = 2048
auto_compact_threshold_percent = 50
max_retries = 2
inference_idle_timeout_secs = 5
agent_type = "grok-build-plan"

[cli]
auto_update = false
CFG

run_lumen() {
  local label="$1" debug_file="$2" out_file="$3" err_file="$4"
  shift 4
  (
    export HOME="$CASE_DIR/home"
    export GROK_HOME="$CASE_DIR/grok-home"
    export LUMEN_HOME="$CASE_DIR/grok-home"
    export LUMEN_FIXTURE_API_KEY="fixture-not-a-secret"
    export GROK_DISABLE_AUTOUPDATER=1
    export GROK_TELEMETRY_ENABLED=false
    export GROK_FEEDBACK_ENABLED=false
    export GROK_TRACE_UPLOAD=false
    export GROK_INSTRUMENTATION=disabled
    unset DEEPSEEK_API_KEY OPENAI_API_KEY ANTHROPIC_API_KEY ANTHROPIC_AUTH_TOKEN
    unset XAI_API_KEY GROK_CODE_XAI_API_KEY GROK_API_KEY GROK_AUTH GROK_AUTH_PATH
    exec "$BIN" \
      --model l5-fixture \
      --cwd "$CASE_DIR/workspace" \
      --output-format plain \
      --always-approve \
      --max-turns 8 \
      --disable-web-search \
      --no-subagents \
      --no-memory \
      --no-plan \
      --system-prompt-override "You are a deterministic L5 contract client. Execute fixture tool calls, preserve the session marker, and do not ask questions." \
      --debug-file "$debug_file" \
      "$@"
  ) >"$out_file" 2>"$err_file" &
  local pid=$! ticks=$((CASE_TIMEOUT_SECONDS * 10)) i
  for ((i = 0; i < ticks; i++)); do
    if ! kill -0 "$pid" 2>/dev/null; then
      set +e
      wait "$pid"
      local ec=$?
      set -e
      if [[ "$ec" -ne 0 ]]; then
        echo "FAIL: $label exited $ec" >&2
        tail -80 "$err_file" "$out_file" >&2 || true
        return 1
      fi
      return 0
    fi
    sleep 0.1
  done
  kill -TERM "$pid" 2>/dev/null || true
  sleep 0.5
  kill -KILL "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  echo "FAIL: $label exceeded ${CASE_TIMEOUT_SECONDS}s" >&2
  return 1
}

echo "=== L5 short A: create session, force auto-compaction, persist checkpoint ==="
run_lumen \
  seed \
  "$CASE_DIR/seed.debug.log" \
  "$CASE_DIR/seed.out.log" \
  "$CASE_DIR/seed.err.log" \
  --session-id "$SESSION_ID" \
  --single "L5_SEED: use the fixture terminal call, then continue after compaction."

[[ -f "$MARKER_FILE" ]] || { echo "FAIL: L5 seed marker missing" >&2; exit 1; }
[[ "$(cat "$MARKER_FILE")" == "$SESSION_TOKEN" ]] || {
  echo "FAIL: L5 seed marker mismatch" >&2
  exit 1
}

SESSION_DIR="$(find "$CASE_DIR/grok-home/sessions" -type d -name "$SESSION_ID" -print -quit 2>/dev/null || true)"
[[ -n "$SESSION_DIR" && -d "$SESSION_DIR" ]] || {
  echo "FAIL: session $SESSION_ID was not persisted" >&2
  exit 1
}
COMPACTION_REQUEST="$(find "$SESSION_DIR/compaction_requests" -type f -name '*.json' -print -quit 2>/dev/null || true)"
COMPACTION_CHECKPOINT="$(find "$SESSION_DIR/compaction_checkpoints" -type f -name '*.json' -print -quit 2>/dev/null || true)"
[[ -n "$COMPACTION_REQUEST" ]] || { echo "FAIL: no persisted compaction request" >&2; exit 1; }
[[ -n "$COMPACTION_CHECKPOINT" ]] || { echo "FAIL: no persisted compaction checkpoint" >&2; exit 1; }
grep -Fq '"kind":"compaction_summary"' "$SERVER_EVENTS"
UPDATES_FILE="$SESSION_DIR/updates.jsonl"
[[ -s "$UPDATES_FILE" ]] || { echo "FAIL: seed updates.jsonl missing" >&2; exit 1; }
read -r SEED_UPDATE_COUNT SEED_EVENT_HIGHWATER < <(
  "$PYTHON_BIN" - "$UPDATES_FILE" "$SESSION_ID" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8")]
session_id = sys.argv[2]
ids = [
    row.get("params", {}).get("_meta", {}).get("eventId")
    for row in rows
    if row.get("params", {}).get("_meta", {}).get("eventId")
]
assert ids, "seed emitted no eventIds"
assert all(event_id.rsplit("-", 1)[0] == session_id for event_id in ids), ids
seqs = [int(event_id.rsplit("-", 1)[1]) for event_id in ids]
print(len(rows), max(seqs))
PY
)

echo "=== L5 short B: explicit resume of the same session + cached-token accounting ==="
run_lumen \
  resume \
  "$CASE_DIR/resume.debug.log" \
  "$CASE_DIR/resume.out.log" \
  "$CASE_DIR/resume.err.log" \
  --resume "$SESSION_ID" \
  --single "L5_RESUME_CHECK: read session_token.txt with a tool and report that the session resumed."

[[ "$(cat "$MARKER_FILE")" == "$SESSION_TOKEN" ]] || {
  echo "FAIL: resume changed the durable marker" >&2
  exit 1
}
grep -Fq '"kind":"l5_read_tool"' "$SERVER_EVENTS"
"$PYTHON_BIN" - \
  "$CASE_DIR/seed.debug.log" \
  "$CASE_DIR/resume.debug.log" \
  "$SESSION_DIR/chat_history.jsonl" \
  "$UPDATES_FILE" \
  "$SESSION_ID" \
  "$SEED_UPDATE_COUNT" \
  "$SEED_EVENT_HIGHWATER" <<'PY'
import json, re, sys
seed, resume, history_path, updates_path, session_id, seed_count, seed_highwater = sys.argv[1:]
logs = "\n".join(open(path, encoding="utf-8", errors="replace").read() for path in (seed, resume))
cache = [int(value) for value in re.findall(r"(?:cache_read_tokens|cached_prompt_tokens)=(\d+)", logs)]
assert any(value > 0 for value in cache), cache
history = [json.loads(line) for line in open(history_path, encoding="utf-8")]
tools = [call.get("name") for row in history for call in (row.get("tool_calls") or [])]
assert "read_file" in tools, tools
updates = [json.loads(line) for line in open(updates_path, encoding="utf-8")]
seed_count = int(seed_count)
seed_highwater = int(seed_highwater)
all_ids = [
    row.get("params", {}).get("_meta", {}).get("eventId")
    for row in updates
    if row.get("params", {}).get("_meta", {}).get("eventId")
]
resume_ids = [
    row.get("params", {}).get("_meta", {}).get("eventId")
    for row in updates[seed_count:]
    if row.get("params", {}).get("_meta", {}).get("eventId")
]
assert resume_ids, "resume emitted no eventIds"
assert len(all_ids) == len(set(all_ids)), all_ids
assert all(event_id.rsplit("-", 1)[0] == session_id for event_id in all_ids), all_ids
resume_seqs = [int(event_id.rsplit("-", 1)[1]) for event_id in resume_ids]
assert min(resume_seqs) > seed_highwater, (seed_highwater, resume_ids)
PY

echo "=== L5 after-seq bounded replay/dedup contract ==="
(
  cd "$ROOT/agent"
  cargo test -p xai-grok-pager duplicate_event_id_is_dropped_and_highwater_advances --lib
  cargo test -p xai-grok-pager reconnect_reload_cursor_tail_appends_to_kept_transcript --lib
)

SOAK_TURNS=0
SOAK_STARTED_AT=""
SOAK_FINISHED_AT=""
SOAK_ELAPSED_SECONDS=0
if [[ "$MODE" == "soak" ]]; then
  echo "=== L5 soak: explicit ${SOAK_SECONDS}s run (minimum ${MIN_SOAK_SECONDS}s) ==="
  SOAK_STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  soak_start_epoch="$(date +%s)"
  while true; do
    now="$(date +%s)"
    elapsed=$((now - soak_start_epoch))
    [[ "$elapsed" -ge "$SOAK_SECONDS" ]] && break
    SOAK_TURNS=$((SOAK_TURNS + 1))
    run_lumen \
      "soak turn $SOAK_TURNS" \
      "$CASE_DIR/soak-${SOAK_TURNS}.debug.log" \
      "$CASE_DIR/soak-${SOAK_TURNS}.out.log" \
      "$CASE_DIR/soak-${SOAK_TURNS}.err.log" \
      --resume "$SESSION_ID" \
      --single "L5_SOAK_TURN_${SOAK_TURNS}: read session_token.txt with a tool; do not modify it."
    [[ "$(cat "$MARKER_FILE")" == "$SESSION_TOKEN" ]] || {
      echo "FAIL: soak turn $SOAK_TURNS changed the marker" >&2
      exit 1
    }
    now="$(date +%s)"
    remaining=$((SOAK_SECONDS - (now - soak_start_epoch)))
    if [[ "$remaining" -gt 0 ]]; then
      sleep_for="$SOAK_INTERVAL_SECONDS"
      [[ "$remaining" -lt "$sleep_for" ]] && sleep_for="$remaining"
      sleep "$sleep_for"
    fi
  done
  SOAK_ELAPSED_SECONDS=$(($(date +%s) - soak_start_epoch))
  SOAK_FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  [[ "$SOAK_ELAPSED_SECONDS" -ge "$MIN_SOAK_SECONDS" ]] || {
    echo "FAIL: soak elapsed ${SOAK_ELAPSED_SECONDS}s < ${MIN_SOAK_SECONDS}s" >&2
    exit 1
  }
fi

"$PYTHON_BIN" - \
  "$ARTIFACT" \
  "$BIN" \
  "$SESSION_ID" \
  "$SESSION_DIR" \
  "$COMPACTION_REQUEST" \
  "$COMPACTION_CHECKPOINT" \
  "$CASE_DIR/seed.debug.log" \
  "$CASE_DIR/resume.debug.log" \
  "$MODE" \
  "$SOAK_SECONDS" \
  "$SOAK_TURNS" \
  "$SOAK_ELAPSED_SECONDS" \
  "$SOAK_STARTED_AT" \
  "$SOAK_FINISHED_AT" \
  "$UPDATES_FILE" \
  "$SEED_UPDATE_COUNT" \
  "$CANONICAL_SOAK_ARTIFACT" \
  "$CANONICAL_SOAK_CASE_DIR" \
  "$CASE_DIR" <<'PY'
import hashlib, json, re, sys
from datetime import datetime, timezone
from pathlib import Path

(
    out, binary, session_id, session_dir, compact_request, compact_checkpoint,
    seed_debug, resume_debug, mode, soak_seconds, soak_turns, soak_elapsed,
    soak_started, soak_finished, updates_path, seed_update_count,
    canonical_soak_artifact, canonical_soak_case_dir, selected_case_dir,
) = sys.argv[1:]
binary = Path(binary)
session_dir = Path(session_dir)
logs = Path(seed_debug).read_text(errors="replace") + "\n" + Path(resume_debug).read_text(errors="replace")
cache = [int(value) for value in re.findall(r"(?:cache_read_tokens|cached_prompt_tokens)=(\d+)", logs)]
updates = []
updates_path = Path(updates_path)
if updates_path.exists():
    for line in updates_path.read_text(errors="replace").splitlines():
        try:
            updates.append(json.loads(line))
        except json.JSONDecodeError:
            pass
event_ids = [
    row.get("params", {}).get("_meta", {}).get("eventId")
    for row in updates
    if row.get("params", {}).get("_meta", {}).get("eventId")
]
seed_update_count = int(seed_update_count)
seed_event_ids = [
    row.get("params", {}).get("_meta", {}).get("eventId")
    for row in updates[:seed_update_count]
    if row.get("params", {}).get("_meta", {}).get("eventId")
]
post_seed_event_ids = [
    row.get("params", {}).get("_meta", {}).get("eventId")
    for row in updates[seed_update_count:]
    if row.get("params", {}).get("_meta", {}).get("eventId")
]
assert seed_event_ids
assert post_seed_event_ids
assert all(event_id.rsplit("-", 1)[0] == session_id for event_id in event_ids)
seed_event_seqs = [int(event_id.rsplit("-", 1)[1]) for event_id in seed_event_ids]
post_seed_event_seqs = [int(event_id.rsplit("-", 1)[1]) for event_id in post_seed_event_ids]
seed_event_highwater = max(seed_event_seqs)
post_seed_event_lowwater = min(post_seed_event_seqs)
soak_executed = mode == "soak"
artifact = {
    "schema_version": 1,
    "check_id": "L5_full",
    "pass": True,
    "scope": "full_contract_soak" if soak_executed else "full_contract_short",
    "binary": str(binary),
    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "session_id": session_id,
    "resume_same_session": session_dir.name == session_id,
    "compaction_request": compact_request,
    "compaction_checkpoint": compact_checkpoint,
    "compaction_persisted": Path(compact_request).is_file() and Path(compact_checkpoint).is_file(),
    "cache_read_tokens_samples": cache[:20],
    "cache_visible": any(value > 0 for value in cache),
    "update_event_id_count": len(event_ids),
    "update_event_ids_unique": len(event_ids) == len(set(event_ids)),
    "event_id_resume_contract": {
        "seed_update_count": seed_update_count,
        "seed_event_highwater": seed_event_highwater,
        "post_seed_event_lowwater": post_seed_event_lowwater,
        "post_seed_all_above_seed_highwater": post_seed_event_lowwater > seed_event_highwater,
    },
    "after_seq_contract_tests": [
        "duplicate_event_id_is_dropped_and_highwater_advances",
        "reconnect_reload_cursor_tail_appends_to_kept_transcript",
    ],
    "runtime_resume_contract": "real persisted eventId highwater continues across a new --resume process",
    "artifact_contract": {
        "selected_artifact": str(Path(out)),
        "selected_case_dir": selected_case_dir,
        "canonical_soak_artifact": canonical_soak_artifact,
        "canonical_soak_case_dir": canonical_soak_case_dir,
        "short_must_not_overwrite_canonical": True,
    },
    "soak": {
        "executed": soak_executed,
        "required_seconds": 3600,
        "requested_seconds": int(soak_seconds),
        "elapsed_seconds": int(soak_elapsed),
        "resume_turns": int(soak_turns),
        "started_at": soak_started or None,
        "finished_at": soak_finished or None,
    },
    "note": (
        "Short mode proves deterministic compaction/resume/cache/after-seq paths but does not claim the one-hour soak."
        if not soak_executed
        else "Explicit soak mode ran for at least one hour and repeatedly resumed the same session."
    ),
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
}
assert artifact["resume_same_session"]
assert artifact["compaction_persisted"]
assert artifact["cache_visible"]
assert artifact["update_event_ids_unique"]
assert artifact["event_id_resume_contract"]["post_seed_all_above_seed_highwater"]
Path(out).write_text(json.dumps(artifact, indent=2) + "\n")
print(json.dumps({"pass": True, "check_id": "L5_full", "scope": artifact["scope"], "artifact": out}))
PY

if [[ "$MODE" == "short" ]]; then
  echo "OK: L5 full short contract passed; one-hour soak NOT RUN; artifact=$ARTIFACT"
  echo "To run the release soak: LUMEN_L5_MODE=soak LUMEN_L5_SOAK_SECONDS=3600 ./scripts/smoke-deepseek-l5.sh"
else
  echo "OK: L5 full soak contract passed (${SOAK_ELAPSED_SECONDS}s, ${SOAK_TURNS} resume turns); artifact=$ARTIFACT"
fi
