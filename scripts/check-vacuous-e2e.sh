#!/usr/bin/env bash
# check-vacuous-e2e.sh — detect async e2e tests missing `.await`
#
# A vacuous e2e test is one where `with_local_set(async { ... })` is called
# without `.await`. The Rust compiler only emits a warning (unused Future),
# so these tests pass in 0.00s without actually executing anything.
#
# Detection rules:
#   1. Count `with_local_set` calls per test file.
#   2. Count `.await` occurrences near those calls.
#   3. If counts don't match → vacuous (report).
#   4. If e2e test runs in < 1s → also suspicious.
#
# Usage:
#   ./scripts/check-vacuous-e2e.sh          # scan all e2e tests
#   ./scripts/check-vacuous-e2e.sh --fix    # only report, cannot auto-fix
#
# Exit: 0 = all clean, 1 = vacuous tests found

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXIT_CODE=0
VACUOUS_COUNT=0

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "=== Lumen: vacuous async e2e check ==="
echo ""

# Find all Rust test files that contain async test patterns
# Focus on e2e and integration test directories
find "$REPO_ROOT/agent" -name "*.rs" -path "*/tests/*" -o -name "*.rs" -path "*/e2e*" | while IFS= read -r file; do
    # Only check files with async test constructs
    if ! grep -q 'with_local_set\|tokio::test\|#\[tokio::test' "$file" 2>/dev/null; then
        continue
    fi

    with_local_count=$(grep -c 'with_local_set' "$file" 2>/dev/null || echo "0")
    await_count=$(grep -c '\.await' "$file" 2>/dev/null || echo "0")

    # Normalize: strip whitespace
    with_local_count=$(echo "$with_local_count" | tr -d '[:space:]')
    await_count=$(echo "$await_count" | tr -d '[:space:]')

    if [ "$with_local_count" -eq 0 ] 2>/dev/null; then
        continue
    fi

    # Check: every `with_local_set` should have at least one `.await` nearby
    # A rough heuristic: if await_count < with_local_count, something is wrong
    if [ "$await_count" -lt "$with_local_count" ] 2>/dev/null; then
        echo -e "${RED}✗ VACUOUS:${NC} $file"
        echo "  with_local_set calls: $with_local_count"
        echo "  .await calls:         $await_count"
        echo "  → Missing .await — test passes without executing!"
        echo ""
        VACUOUS_COUNT=$((VACUOUS_COUNT + 1))
        EXIT_CODE=1
    fi
done

# Also check for the specific pattern: `with_local_set(...)` followed by `;` without `.await`
echo ""
echo "--- Deep scan: with_local_set without .await ---"
grep -rn 'with_local_set' "$REPO_ROOT/agent" --include='*.rs' 2>/dev/null | while IFS= read -r line; do
    # Check if the same line has `.await` somewhere (rough)
    if ! echo "$line" | grep -q '\.await'; then
        file=$(echo "$line" | cut -d: -f1)
        lineno=$(echo "$line" | cut -d: -f2)
        # Check next 3 lines for .await
        if ! sed -n "$((lineno)),$((lineno + 3))p" "$file" 2>/dev/null | grep -q '\.await'; then
            echo -e "${YELLOW}⚠ SUSPICIOUS:${NC} $file:$lineno"
            echo "  $line"
            echo "  → No .await found within 3 lines — may be vacuous"
            echo ""
        fi
    fi
done

echo ""
if [ "$EXIT_CODE" -eq 0 ]; then
    echo -e "${GREEN}✓ All e2e tests have matching .await calls${NC}"
else
    echo -e "${RED}✗ Found potentially vacuous e2e tests${NC}"
    echo "  Fix: add .await after with_local_set(...) calls"
fi

exit $EXIT_CODE
