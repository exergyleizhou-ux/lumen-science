#!/usr/bin/env bash
# M2 discipline smoke: lumen-discipline unit tests + structural wiring.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"

echo "=== lumen-discipline tests (storm / delivery / cache) ==="
(
  cd "$ROOT/agent"
  cargo test -p lumen-discipline --lib -- --nocapture
)

echo "=== structural: update_goal gate + presets ==="
UG="$ROOT/agent/crates/codegen/xai-grok-tools/src/implementations/grok_build/update_goal/mod.rs"
grep -q 'lumen_goal_incomplete_gate' "$UG" || {
  echo "FAIL: goal incomplete gate not wired" >&2
  exit 1
}
grep -q 'incomplete_todos\|gate_goal_complete' "$UG" || {
  echo "FAIL: gate_goal_complete missing from update_goal" >&2
  exit 1
}
MODELS="$ROOT/agent/crates/codegen/xai-grok-models/default_models.json"
grep -q 'deepseek-v4-pro' "$MODELS" || {
  echo "FAIL: deepseek-v4-pro formal preset missing" >&2
  exit 1
}
grep -q 'local-openai\|local-model' "$MODELS" || {
  echo "FAIL: local preset missing" >&2
  exit 1
}
test -f "$ROOT/docs/masterplan/08-M2-循环纪律.md" || {
  echo "FAIL: missing M2 design doc" >&2
  exit 1
}

echo "OK: smoke-m2 passed"
exit 0
