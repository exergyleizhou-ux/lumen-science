#!/usr/bin/env bash
# L2 min agent E2E: read → write/edit → bash on isolated workspace (FINAL-2.0).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"

KEY="${DEEPSEEK_API_KEY:-}"
if [[ -z "$KEY" ]]; then
  echo "SKIP: DEEPSEEK_API_KEY required for L2"
  exit 2
fi

# Unconditionally ask Cargo to validate/rebuild this checkout. Finding an old
# release executable is not evidence that L2 exercised the current source.
(cd "$ROOT/agent" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
  cargo build --locked -p xai-grok-pager-bin --release)
BIN="$ROOT/agent/target/release/lumen"
test -x "$BIN"
BIN_SHA="$(shasum -a 256 "$BIN" | awk '{print $1}')"

WS="$(mktemp -d "${TMPDIR:-/tmp}/lumen-l2-ws-XXXXXX")"
HOME_ISO="$(mktemp -d "${TMPDIR:-/tmp}/lumen-l2-home-XXXXXX")"
PROOF="$ROOT/artifacts/readiness"
mkdir -p "$PROOF"
DEBUG="$HOME_ISO/debug.log"
OUT="$HOME_ISO/out.txt"

# Workspace fixture: seed file to read; target must become EXACT success line (no pre-seed of marker).
printf 'seed=L2_SEED_VALUE\n' >"$WS/input.txt"
printf 'PLACEHOLDER_UNSET\n' >"$WS/answer.txt"

export GROK_HOME="$HOME_ISO"
export DEEPSEEK_API_KEY="$KEY"
unset XAI_API_KEY GROK_CODE_XAI_API_KEY 2>/dev/null || true

cat >"$HOME_ISO/config.toml" <<'CFG'
[models]
default = "deepseek-v4-pro"
[model.deepseek-v4-pro]
model = "deepseek-v4-pro"
base_url = "https://api.deepseek.com/v1"
api_backend = "chat_completions"
env_key = "DEEPSEEK_API_KEY"
supports_reasoning_effort = true
reasoning_effort = "high"
[cli]
auto_update = false
CFG

PROMPT="Working directory is $WS.
1) Use a file-read tool to read input.txt (must see L2_SEED_VALUE).
2) Use write or search_replace so answer.txt is exactly one line containing only: L2_ANSWER=ok
   (replace PLACEHOLDER_UNSET entirely)
3) Use a bash/terminal tool to run: cat answer.txt
4) Reply with only that cat stdout.
Do not skip tools."

echo "L2 workspace=$WS"
set +e
"$BIN" -m deepseek-v4-pro --cwd "$WS" --single "$PROMPT" \
  --output-format plain --always-approve --max-turns 12 \
  --debug-file "$DEBUG" >"$OUT" 2>&1
EC=$?
set -e

echo "----- out -----"
cat "$OUT"
echo "----- answer.txt -----"
cat "$WS/answer.txt" 2>/dev/null || true

# Evidence
FILE_OK=0
if [[ -f "$WS/answer.txt" ]] && [[ "$(tr -d '\r\n' <"$WS/answer.txt")" == "L2_ANSWER=ok" ]]; then
  FILE_OK=1
fi
TOOL=0
grep -Eq 'Model requesting tool:|stop_reason="tool_calls"|response.has_tool_call=true' "$DEBUG" && TOOL=1 || true
READ_T=0
grep -Eq "Model requesting tool: name='(read_file|read|Read)'|name=\"read" "$DEBUG" && READ_T=1 || true
# broader tool name match
grep -Eiq 'requesting tool: name=.*(read|write|search_replace|run_terminal)' "$DEBUG" && TOOL=1 || true
BASH_T=0
grep -Eiq "requesting tool: name=.*(run_terminal|bash)" "$DEBUG" && BASH_T=1 || true
WRITE_T=0
grep -Eiq "requesting tool: name=.*(write|search_replace|edit)" "$DEBUG" && WRITE_T=1 || true

python3 -c "
import json, re
from datetime import datetime, timezone
from pathlib import Path
out = Path(r'''$OUT''').read_text(errors='replace')
dbg = Path(r'''$DEBUG''').read_text(errors='replace') if Path(r'''$DEBUG''').exists() else ''
red = lambda s: re.sub(r'sk-[a-zA-Z0-9]+', 'sk-REDACTED', s)
tools = re.findall(r\"Model requesting tool: name='([^']+)'\", dbg)
ok = ($EC == 0) and ($FILE_OK == 1) and ($TOOL == 1) and len(tools) >= 1
art = {
  'schema_version': 1,
  'check_id': 'L2',
  'pass': ok,
  'exit_code': $EC,
  'binary': r'''$BIN''',
  'binary_sha256': '$BIN_SHA',
  'file_answer_ok': bool($FILE_OK),
  'tool_count': len(tools),
  'tools': tools[:20],
  'had_read_like': bool($READ_T),
  'had_write_like': bool($WRITE_T),
  'had_bash_like': bool($BASH_T),
  'generated_at': datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
  'output_snippet': red(out)[:500],
  'answer_txt': Path(r'''$WS/answer.txt''').read_text(errors='replace')[:200] if Path(r'''$WS/answer.txt''').exists() else '',
}
Path(r'''$PROOF/L2-min-e2e.json''').write_text(json.dumps(art, indent=2)+'\n')
print(json.dumps({'pass': ok, 'tools': tools, 'file_ok': bool($FILE_OK)}, indent=2))
raise SystemExit(0 if ok else 1)
"
PEC=$?
# keep workspace path out of repo; proof is enough
rm -rf "$WS" "$HOME_ISO"
if [[ $PEC -ne 0 ]]; then
  echo "FAIL: L2 min e2e" >&2
  exit 1
fi
echo "OK: L2 min e2e signed binary_sha256=$BIN_SHA"
exit 0
