#!/usr/bin/env bash
# Day 0 acceptance gates — must exit 0 for foundation slice.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() { echo "FAIL: $*" >&2; exit 1; }

test -d "$ROOT/agent/crates/codegen/xai-grok-pager-bin" || fail "missing agent/crates/codegen/xai-grok-pager-bin"
test -f "$ROOT/NOTICE" || fail "missing NOTICE"
test -f "$ROOT/LEGAL.md" || fail "missing LEGAL.md"
test -f "$ROOT/agent/UPSTREAM.md" || fail "missing agent/UPSTREAM.md"
test -f "$ROOT/policy/CC_PARITY.md" || fail "missing policy/CC_PARITY.md"
test -f "$ROOT/docs/masterplan/00-终极决议.md" || fail "missing masterplan"
test -f "$ROOT/journal/2026-07-16-day0.md" || fail "missing day0 journal"

git -C "$ROOT" rev-parse HEAD >/dev/null || fail "not a git repo"
COMMITS=$(git -C "$ROOT" rev-list --count HEAD)
test "$COMMITS" -ge 1 || fail "no commits"

# Structural: pager-bin is a workspace package
grep -q 'name = "xai-grok-pager-bin"' \
  "$ROOT/agent/crates/codegen/xai-grok-pager-bin/Cargo.toml" \
  || fail "pager-bin Cargo.toml name mismatch"

echo "OK: Day 0 layout + git gates pass (ROOT=$ROOT commits=$COMMITS)"
