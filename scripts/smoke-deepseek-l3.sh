#!/usr/bin/env bash
# L3 multi-tool / multi-turn tool use (FINAL-2.0).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"

KEY="${DEEPSEEK_API_KEY:-}"
if [[ -z "$KEY" ]]; then
  echo "SKIP: DEEPSEEK_API_KEY required for L3"
  exit 2
fi

# Unconditionally ask Cargo to validate/rebuild this checkout. Finding an old
# release executable is not evidence that L3 exercised the current source.
(cd "$ROOT/agent" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
  cargo build --locked -p xai-grok-pager-bin --release)
BIN="$ROOT/agent/target/release/lumen"
test -x "$BIN"
BIN_SHA="$(shasum -a 256 "$BIN" | awk '{print $1}')"

HOME_ISO="$(mktemp -d "${TMPDIR:-/tmp}/lumen-l3-home-XXXXXX")"
PROOF="$ROOT/artifacts/readiness"
mkdir -p "$PROOF"
DEBUG="$HOME_ISO/debug.log"
OUT="$HOME_ISO/out.txt"
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

PROMPT='You MUST use terminal tools at least TWICE (two separate tool calls):
1) run: echo L3_STEP_A
2) run: echo L3_STEP_B
After both tool results, reply with exactly: L3_STEP_A L3_STEP_B
Do not invent tool results.'

set +e
"$BIN" -m deepseek-v4-pro --single "$PROMPT" --output-format plain --always-approve --max-turns 12 \
  --debug-file "$DEBUG" >"$OUT" 2>&1
EC=$?
set -e
echo "----- out -----"
cat "$OUT"

python3 -c "
import json, re
from datetime import datetime, timezone
from pathlib import Path
out = Path(r'''$OUT''').read_text(errors='replace')
dbg = Path(r'''$DEBUG''').read_text(errors='replace') if Path(r'''$DEBUG''').exists() else ''
red = lambda s: re.sub(r'sk-[a-zA-Z0-9]+', 'sk-REDACTED', s)
tools = re.findall(r\"Model requesting tool: name='([^']+)'\", dbg)
# count tool call ids
call_ids = re.findall(r\"call_id='([^']+)'\", dbg)
n = max(len(tools), len(set(call_ids)))
ok = ($EC == 0) and n >= 2 and ('L3_STEP_A' in out) and ('L3_STEP_B' in out)
art = {
  'schema_version': 1,
  'check_id': 'L3',
  'pass': ok,
  'exit_code': $EC,
  'binary': r'''$BIN''',
  'binary_sha256': '$BIN_SHA',
  'tool_request_count': n,
  'tools': tools[:30],
  'call_ids': list(dict.fromkeys(call_ids))[:30],
  'generated_at': datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
  'output_snippet': red(out)[:500],
}
Path(r'''$PROOF/L3-multi-tool.json''').write_text(json.dumps(art, indent=2)+'\n')
print(json.dumps({'pass': ok, 'tool_request_count': n, 'tools': tools}, indent=2))
raise SystemExit(0 if ok else 1)
"
PEC=$?
rm -rf "$HOME_ISO"
[[ $PEC -eq 0 ]] || { echo "FAIL: L3 multi-tool" >&2; exit 1; }
echo "OK: L3 multi-tool signed binary_sha256=$BIN_SHA"
exit 0
