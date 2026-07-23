#!/usr/bin/env bash
# R0-min: drive real shipped process-group kill/idempotency tests (xai-tty-utils).
# Plus structural contract markers for session tool_calls / cancel.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"
PROOF="$ROOT/artifacts/readiness"
mkdir -p "$PROOF"

echo "=== R0-min: xai-tty-utils ProcessScope kill_all (real shipped code) ==="
(
  cd "$ROOT/agent"
  cargo test -p xai-tty-utils --lib kill_all -- --nocapture
)

echo "=== R0-min: structural session contract markers ==="
SESS="$ROOT/agent/crates/codegen/xai-grok-shell/src/session"
test -d "$SESS"
grep -Rql --include='*.rs' 'tool_calls' "$SESS"
grep -Rql --include='*.rs' 'cancel' "$SESS" || grep -Rql --include='*.rs' 'CancellationToken' "$ROOT/agent/crates/codegen/xai-grok-shell/src"

# Idempotency test names must exist in tty-utils
grep -q 'kill_all_is_idempotent' \
  "$ROOT/agent/crates/codegen/xai-tty-utils/src/process_scope.rs"

python3 -c "
import json
from datetime import datetime, timezone
from pathlib import Path
art = {
  'schema_version': 1,
  'check_id': 'R0_min',
  'pass': True,
  'scope': 'minimal',
  'driven': [
    'cargo test -p xai-tty-utils --lib kill_all (kill_all_reaps / kill_all_is_idempotent)',
    'structural: session tool_calls + cancel markers',
  ],
  'residual_full_r0': [
    'R4 after_seq bounded replay e2e',
    'R6 kill -9 whole agent process recovery e2e',
    'R7 full UI reconcile',
  ],
  'generated_at': datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
  'note': 'R0_min signs cancel/idempotent process teardown via real xai-tty-utils; not full R0 matrix',
}
Path(r'''$PROOF/R0-min.json''').write_text(json.dumps(art, indent=2)+'\n')
print('wrote R0-min.json pass=True')
"
echo "OK: R0-min signed"
exit 0
