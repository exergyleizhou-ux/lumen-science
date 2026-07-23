#!/usr/bin/env bash
# eval-coding.sh — Coding eval harness for ≥20 tasks (Masterplan M4).
# Usage:
#   ./scripts/eval-coding.sh                    # verify all broken tasks (harness-only)
#   ./scripts/eval-coding.sh --model deepseek   # reserved; not required for harness-only
#
# macOS-safe: does NOT depend on GNU `timeout`.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TASKS="$ROOT/evals/tasks"
export PATH="${HOME}/.local/bin:/opt/homebrew/bin:${PATH:-}"

PASS=0
FAIL=0
SKIP=0
TOTAL=0
BUILD_ERR=0

echo "=== lumen coding eval harness ==="
echo "Tasks dir: $TASKS"
echo ""

# Portable run: capture stdout+stderr and exit code without GNU timeout.
run_capture() {
  local dir="$1"
  shift
  local outf ec
  outf="$(mktemp)"
  set +e
  (
    cd "$dir" || exit 127
    "$@"
  ) >"$outf" 2>&1
  ec=$?
  set -e
  cat "$outf"
  rm -f "$outf"
  return "$ec"
}

for task_dir in "$TASKS"/*/; do
  name=$(basename "$task_dir")
  ws="$task_dir/workspace"
  TOTAL=$((TOTAL + 1))

  if [[ ! -d "$ws" ]]; then
    echo "  SKIP $name: no workspace/"
    SKIP=$((SKIP + 1))
    continue
  fi

  if [[ -f "$ws/go.mod" ]]; then
    set +e
    # Use Go's built-in -timeout (portable; do not use GNU timeout(1)).
    out=$(run_capture "$ws" go test ./... -count=1 -timeout 30s)
    ec=$?
    set -e
    if [[ $ec -eq 0 ]]; then
      echo "  PASS $name (go test) — unexpected for broken workspace"
      PASS=$((PASS + 1))
    else
      if echo "$out" | grep -qE 'FAIL|--- FAIL:|undefined:|build failed|error:'; then
        echo "  BROKEN $name (go test) — OK: harness detects failure"
        FAIL=$((FAIL + 1))
      else
        echo "  BUILD-ERR $name (go test) — $(echo "$out" | head -1)"
        BUILD_ERR=$((BUILD_ERR + 1))
        FAIL=$((FAIL + 1))
      fi
    fi
  elif [[ -f "$ws/pytest.ini" ]] || find "$ws" -name 'test_*.py' -o -name '*_test.py' 2>/dev/null | grep -q .; then
    if command -v python3 &>/dev/null; then
      set +e
      out=$(run_capture "$ws" python3 -m pytest -q)
      ec=$?
      set -e
      if [[ $ec -eq 0 ]]; then
        echo "  PASS $name (pytest) — unexpected for broken workspace"
        PASS=$((PASS + 1))
      else
        echo "  BROKEN $name (pytest) — OK: harness detects failure"
        FAIL=$((FAIL + 1))
      fi
    else
      echo "  SKIP $name (python3 not found)"
      SKIP=$((SKIP + 1))
    fi
  elif [[ -f "$ws/package.json" ]]; then
    if command -v npx &>/dev/null; then
      set +e
      out=$(run_capture "$ws" npx --yes vitest run)
      ec=$?
      set -e
      if [[ $ec -eq 0 ]]; then
        echo "  PASS $name (vitest) — unexpected for broken workspace"
        PASS=$((PASS + 1))
      else
        echo "  BROKEN $name (vitest) — OK: harness detects failure"
        FAIL=$((FAIL + 1))
      fi
    else
      echo "  SKIP $name (npx not found)"
      SKIP=$((SKIP + 1))
    fi
  else
    echo "  SKIP $name: unknown project type"
    SKIP=$((SKIP + 1))
  fi
done

echo ""
echo "=== Results ==="
echo "Total: $TOTAL"
echo "BROKEN (expected): $FAIL"
echo "Pass (unexpected fix): $PASS"
echo "Build-err: $BUILD_ERR"
echo "Skipped: $SKIP"

# Harness-only success: every non-skipped task must be BROKEN (fail tests), zero unexpected PASS.
if [[ "$TOTAL" -lt 20 ]]; then
  echo "FAIL: need ≥20 tasks, got $TOTAL" >&2
  exit 1
fi
if [[ "$PASS" -gt 0 ]]; then
  echo "FAIL: $PASS tasks already pass — workspaces must be broken for coding eval" >&2
  exit 1
fi
if [[ "$SKIP" -gt 0 ]]; then
  echo "FAIL: $SKIP tasks skipped — install missing toolchains or fix detection" >&2
  exit 1
fi
if [[ "$FAIL" -lt 20 ]]; then
  echo "FAIL: expected 20 broken detections, got $FAIL" >&2
  exit 1
fi

echo "OK: harness verification complete (20 broken workspaces detected)"
exit 0
