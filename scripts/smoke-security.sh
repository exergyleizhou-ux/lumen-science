#!/usr/bin/env bash
# M1 security smoke — Lumen L0–L3 hard-deny (lumen-guard).
# Exit 0 only when all masterplan minimum cases deny as expected.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"

echo "=== lumen-guard unit tests (includes masterplan smoke cases) ==="
(
  cd "$ROOT/agent"
  cargo test -p lumen-guard --lib -- --nocapture
)

echo "=== structural: guard wired into permission manager ==="
MGR="$ROOT/agent/crates/codegen/xai-grok-workspace/src/permission/manager.rs"
grep -q 'lumen_guard_deny' "$MGR" || {
  echo "FAIL: lumen_guard_deny not found in permission manager" >&2
  exit 1
}
grep -q 'LUMEN_GUARD_DENY' "$MGR" || {
  echo "FAIL: LUMEN_GUARD_DENY reason constant missing" >&2
  exit 1
}
grep -q 'lumen-guard' "$ROOT/agent/crates/codegen/xai-grok-workspace/Cargo.toml" || {
  echo "FAIL: workspace crate missing lumen-guard dependency" >&2
  exit 1
}

echo "=== policy doc present ==="
test -f "$ROOT/policy/GUARD_RULES.md" || {
  echo "FAIL: missing policy/GUARD_RULES.md" >&2
  exit 1
}

echo "OK: smoke-security passed (lumen-guard tests + wiring + policy)"
exit 0
