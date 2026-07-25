#!/usr/bin/env bash
# lumen-e2e.sh — run Lumen e2e tests with guaranteed fresh binary
#
# The e2e test harness spawns target/debug/lumen from xai-grok-pager-bin.
# If the binary is stale, tests get "Method not found" (404).
# This wrapper ensures the binary is rebuilt before running e2e tests.
#
# Usage:
#   ./scripts/lumen-e2e.sh                    # build + run all e2e
#   ./scripts/lumen-e2e.sh --test <name>      # build + run specific test
#   ./scripts/lumen-e2e.sh --no-build          # skip build (you know it's fresh)
#
# Env:
#   LUMEN_E2E_TEST_THREADS=4    # override default test thread count

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
AGENT_DIR="$REPO_ROOT/agent"
TEST_THREADS="${LUMEN_E2E_TEST_THREADS:-4}"
BUILD=true
TEST_FILTER=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-build) BUILD=false; shift ;;
        --test) TEST_FILTER="$2"; shift 2 ;;
        *) echo "Unknown flag: $1"; exit 1 ;;
    esac
done

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "=== Lumen E2E Test Runner ==="
echo ""

if $BUILD; then
    echo -e "${YELLOW}Building pager binary...${NC}"
    cd "$AGENT_DIR"
    cargo build -p xai-grok-pager-bin 2>&1 | tail -3
    echo -e "${GREEN}✓ Binary built${NC}"
    echo ""
fi

# Verify binary exists
BINARY="$AGENT_DIR/target/debug/lumen"
if [[ ! -f "$BINARY" ]]; then
    echo "ERROR: Binary not found at $BINARY"
    echo "Run without --no-build to build it first."
    exit 1
fi

# Check binary freshness vs source
BIN_MTIME=$(stat -f %m "$BINARY" 2>/dev/null || echo "0")
echo "Binary: $BINARY (modified: $(date -r "$BIN_MTIME" '+%Y-%m-%d %H:%M:%S' 2>/dev/null || echo "unknown"))"

echo ""
echo -e "${YELLOW}Running e2e tests...${NC}"

cd "$AGENT_DIR"
if [[ -n "$TEST_FILTER" ]]; then
    cargo test --test-threads="$TEST_THREADS" -- "$TEST_FILTER" 2>&1
else
    cargo test --test-threads="$TEST_THREADS" 2>&1
fi

EXIT=$?
echo ""
if [[ $EXIT -eq 0 ]]; then
    echo -e "${GREEN}✓ All e2e tests passed${NC}"
else
    echo "✗ E2e tests failed (exit: $EXIT)"
fi
exit $EXIT
