#!/usr/bin/env bash
# smoke-verify.sh — Go self-repair path demo for lumen-verify (M4 exit).
# 1) bad Go file → verify fail
# 2) fixed Go file → verify ok
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="${HOME}/.local/bin:/opt/homebrew/bin:${HOME}/.cargo/bin:${PATH:-}"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"

echo "=== lumen-verify unit tests ==="
(
  cd "$ROOT/agent"
  cargo test -p lumen-verify --lib --quiet
)

echo "=== event sequence restore unit tests ==="
(
  cd "$ROOT/agent"
  cargo test -p xai-grok-shell-base --lib util::event_id::tests --quiet
)

echo "=== M5 onboarding evidence contract ==="
"$ROOT/scripts/test-onboarding-gate.sh"

echo "=== real agent writer -> verify feedback -> repair registry tests ==="
(
  cd "$ROOT/agent"
  cargo test -p xai-grok-tools \
    registry::types::tests::hub_dispatch_search_replace_auto_verifies_broken_then_fixed_go \
    -- --exact --quiet
  cargo test -p xai-grok-tools \
    registry::types::tests::hub_dispatch_write_auto_verifies_go_output \
    -- --exact --quiet
)

echo "=== build lumen-verify CLI ==="
(
  cd "$ROOT/agent"
  cargo build -p lumen-verify --bin lumen-verify --quiet
)
BIN="$ROOT/agent/target/debug/lumen-verify"
test -x "$BIN"

DEMO="$(mktemp -d "${TMPDIR:-/tmp}/lumen-verify-demo-XXXXXX")"
cleanup() { rm -rf "$DEMO"; }
trap cleanup EXIT

mkdir -p "$DEMO"
cat >"$DEMO/go.mod" <<'EOF'
module verifydemo

go 1.22
EOF

echo "=== case 1: broken Go must FAIL ==="
cat >"$DEMO/main.go" <<'EOF'
package main

func main() {
	// deliberate compile error
	x := undefinedName
	_ = x
}
EOF
set +e
"$BIN" --root "$DEMO" --changed main.go >/tmp/lv-bad.out 2>/tmp/lv-bad.err
ec=$?
set -e
if [[ $ec -eq 0 ]]; then
  echo "FAIL: expected non-zero exit on broken Go" >&2
  cat /tmp/lv-bad.err /tmp/lv-bad.out >&2
  exit 1
fi
echo "OK: broken Go → exit $ec (diagnostics:)"
head -20 /tmp/lv-bad.err || true

echo "=== case 2: fixed Go must PASS ==="
cat >"$DEMO/main.go" <<'EOF'
package main

func main() {}
EOF
set +e
"$BIN" --root "$DEMO" --changed main.go >/tmp/lv-ok.out 2>/tmp/lv-ok.err
ec=$?
set -e
if [[ $ec -ne 0 ]]; then
  echo "FAIL: expected success on fixed Go" >&2
  cat /tmp/lv-ok.err /tmp/lv-ok.out >&2
  exit 1
fi
echo "OK: fixed Go → exit 0"
echo "OK: smoke-verify passed (CLI bad→fail/good→ok + real writer feedback→repair)"
exit 0
