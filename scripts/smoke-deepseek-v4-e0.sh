#!/usr/bin/env bash
# Product-level DeepSeek V4 E0 smoke. Builds and runs this checkout's Lumen;
# never calls the provider through a side-channel HTTP client and never prints
# credentials or provider response bodies.
set -euo pipefail

MODE="${1:-}"
case "$MODE" in
  --expect-401|--live) ;;
  *) echo "usage: $0 --expect-401|--live" >&2; exit 64 ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"

# Always ask Cargo to validate/rebuild the current checkout. Merely finding an
# executable in target/ is not evidence that it contains the current source.
(cd "$ROOT/agent" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
  cargo build --locked -p xai-grok-pager-bin)
BIN="$ROOT/agent/target/debug/lumen"
test -x "$BIN"
BIN_SHA="$(shasum -a 256 "$BIN" | awk '{print $1}')"

SMOKE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/lumen-v4-e0-XXXXXX")"
cleanup() { rm -rf "$SMOKE_ROOT"; }
trap cleanup EXIT
export LUMEN_HOME="$SMOKE_ROOT/lumen-home"
export GROK_HOME="$SMOKE_ROOT/grok-home"
mkdir -p "$LUMEN_HOME" "$GROK_HOME"
unset XAI_API_KEY GROK_CODE_XAI_API_KEY GROK_API_KEY OPENAI_API_KEY OPENAI_BASE_URL 2>/dev/null || true
cat >"$LUMEN_HOME/config.toml" <<'CFG'
[models]
default = "deepseek-v4-pro"
[cli]
auto_update = false
CFG

COMMON=(
  -m deepseek-v4-pro
  --output-format plain
  --always-approve
  --max-turns 8
)

if [[ "$MODE" == "--expect-401" ]]; then
  export DEEPSEEK_API_KEY="lumen-e0-intentionally-invalid"
  OUT="$SMOKE_ROOT/401.out"
  DEBUG="$SMOKE_ROOT/401.debug"
  set +e
  "$BIN" "${COMMON[@]}" --single "Reply with one word." --debug-file "$DEBUG" >"$OUT" 2>&1
  EC=$?
  set -e
  if [[ $EC -eq 0 ]] || ! grep -Eq '401|Unauthorized|Authentication Fails|authentication_error' "$OUT" "$DEBUG"; then
    echo "FAIL: Lumen provider path did not surface the expected 401" >&2
    exit 1
  fi
  if ! grep -Eq 'deepseek-v4-pro|api\.deepseek\.com' "$OUT" "$DEBUG"; then
    echo "FAIL: 401 evidence did not traverse the formal DeepSeek V4 Lumen route" >&2
    exit 1
  fi
  echo "OK: product E0 auth smoke model=deepseek-v4-pro status=401 binary_sha256=$BIN_SHA"
  exit 0
fi

KEY="${DEEPSEEK_API_KEY:-}"
if [[ -z "$KEY" ]]; then
  echo "BLOCKED: DEEPSEEK_API_KEY is absent; product live smoke was not executed" >&2
  exit 2
fi
export DEEPSEEK_API_KEY="$KEY"

EVIDENCE_FILE="$SMOKE_ROOT/tool-executed.txt"
MARKER="LUMEN_E0_TOOL_EXECUTED_20260719"
FIRST_OUT="$SMOKE_ROOT/first.out"
FIRST_DEBUG="$SMOKE_ROOT/first.debug"
FIRST_PROMPT="Use run_terminal_command to execute exactly: printf '%s\\n' '$MARKER' | tee '$EVIDENCE_FILE'. You must use the tool; do not merely describe it."
"$BIN" "${COMMON[@]}" --single "$FIRST_PROMPT" --debug-file "$FIRST_DEBUG" >"$FIRST_OUT" 2>&1

# A model response containing MARKER proves nothing. The side effect, created
# only by Lumen's real tool runtime, is the execution oracle.
if [[ ! -f "$EVIDENCE_FILE" ]] || [[ "$(tr -d '\r\n' <"$EVIDENCE_FILE")" != "$MARKER" ]]; then
  echo "FAIL: Lumen tool runtime did not create the exact evidence file" >&2
  exit 1
fi

CHAT_FILE="$(find "$SMOKE_ROOT" -type f -name chat_history.jsonl -print -quit 2>/dev/null || true)"
if [[ -z "$CHAT_FILE" ]]; then
  echo "FAIL: first product turn created no persisted chat_history.jsonl" >&2
  exit 1
fi
SESSION_ID="$(basename "$(dirname "$CHAT_FILE")")"
case "$SESSION_ID" in
  ????????-????-????-????-????????????) ;;
  *) echo "FAIL: could not resolve a persisted Lumen session id" >&2; exit 1 ;;
esac

SECOND_OUT="$SMOKE_ROOT/second.out"
SECOND_DEBUG="$SMOKE_ROOT/second.debug"
SECOND_MARKER="LUMEN_E0_RESUME_TOOL_EXECUTED_20260719"
SECOND_FILE="$SMOKE_ROOT/resume-tool-executed.txt"
SECOND_PROMPT="This is the resumed next turn. Use run_terminal_command to read '$EVIDENCE_FILE', then use run_terminal_command to execute exactly: printf '%s\\n' '$SECOND_MARKER' | tee '$SECOND_FILE'. Do not invent either tool result."
"$BIN" "${COMMON[@]}" --resume "$SESSION_ID" --single "$SECOND_PROMPT" \
  --debug-file "$SECOND_DEBUG" >"$SECOND_OUT" 2>&1
if [[ ! -f "$SECOND_FILE" ]] || [[ "$(tr -d '\r\n' <"$SECOND_FILE")" != "$SECOND_MARKER" ]]; then
  echo "FAIL: resumed Lumen process did not execute the next-turn tool" >&2
  exit 1
fi

# Verification may parse local evidence, but it does not reproduce provider or
# session logic. Require real JSONL types: reasoning, assistant tool calls, and
# tool results from both the original and resumed process.
python3 - "$CHAT_FILE" "$MARKER" "$SECOND_MARKER" <<'PY'
import json, sys
path, first, second = sys.argv[1:]
items = [json.loads(line) for line in open(path, encoding="utf-8") if line.strip()]
encoded = [json.dumps(item, sort_keys=True) for item in items]
reasoning = sum('"reasoning"' in line or 'reasoning_content' in line for line in encoded)
tool_calls = sum('tool_call' in line and 'tool_result' not in line for line in encoded)
tool_results = [line for line in encoded if 'tool_result' in line]
if reasoning < 2:
    raise SystemExit("FAIL: persisted resumed session lacks two reasoning records")
if tool_calls < 2:
    raise SystemExit("FAIL: persisted resumed session lacks two assistant tool-call records")
if len(tool_results) < 2:
    raise SystemExit("FAIL: persisted resumed session lacks real tool-result records")
if not any(first in line for line in tool_results):
    raise SystemExit("FAIL: first execution marker is absent from persisted tool result")
if not any(second in line for line in tool_results):
    raise SystemExit("FAIL: resumed execution marker is absent from persisted tool result")
PY

if ! grep -Eq 'deepseek-v4-pro|api\.deepseek\.com' "$FIRST_DEBUG" "$SECOND_DEBUG"; then
  echo "FAIL: debug evidence did not show the formal embedded catalog route" >&2
  exit 1
fi

echo "OK: product E0 live smoke model=deepseek-v4-pro tool_execution=true persistence=true reload=true continuation=true binary_sha256=$BIN_SHA"
