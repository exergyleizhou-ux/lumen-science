#!/usr/bin/env bash
# lumen-model-fallback.sh — monitor API health and suggest model switches
#
# Watches Lumen's proxy log for consecutive upstream failures and suggests
# switching to a fallback model when a threshold is reached.
#
# Usage:
#   ./scripts/lumen-model-fallback.sh                 # one-shot check
#   ./scripts/lumen-model-fallback.sh --watch          # continuous monitoring
#   ./scripts/lumen-model-fallback.sh --reset          # clear failure counters
#
# Exit: 0 = healthy, 1 = failures detected, 2 = threshold exceeded

# Cross-platform reverse-line reader (macOS: tail -r, Linux: tac)
_reverse_lines() {
    if command -v tac &>/dev/null; then
        tac "$@"
    else
        tail -r "$@"
    fi
}

LUMEN_DIR="${LUMEN_DIR:-$HOME/.lumen}"
PROXY_LOG="$LUMEN_DIR/science/proxy.log"
STATE_FILE="$LUMEN_DIR/.api_failure_state"
THRESHOLD="${LUMEN_API_FAILURE_THRESHOLD:-3}"

RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
NC='\033[0m'

cmd_reset() {
    rm -f "$STATE_FILE"
    echo "✓ API failure state reset"
}

cmd_check() {
    if [[ ! -f "$PROXY_LOG" ]]; then
        echo "✓ No proxy log found — API appears healthy"
        return 0
    fi

    local failures
    failures=$(grep -c "context canceled\|upstream jitter.*retry" "$PROXY_LOG" 2>/dev/null || echo "0")
    failures=$(echo "$failures" | tr -d '[:space:]')

    if [[ "$failures" -eq 0 ]]; then
        echo -e "${GREEN}✓${NC} API healthy — 0 recent failures"
        return 0
    fi

    # Count consecutive failures (lines after last successful request)
    local consecutive
    if command -v tac &>/dev/null; then
        consecutive=$(tac "$PROXY_LOG" 2>/dev/null | while IFS= read -r line; do
        if echo "$line" | grep -q "context canceled\|upstream jitter"; then
            echo "fail"
        else
            break
        fi
    done | wc -l | tr -d '[:space:]')
    else
        consecutive=$(tail -r "$PROXY_LOG" 2>/dev/null | while IFS= read -r line; do
        if echo "$line" | grep -q "context canceled\|upstream jitter"; then
            echo "fail"
        else
            break
        fi
    done | wc -l | tr -d '[:space:]')
    fi

    if [[ "$consecutive" -ge "$THRESHOLD" ]]; then
        echo -e "${RED}✗${NC} API degraded — ${consecutive} consecutive failures (threshold: ${THRESHOLD})"
        echo ""
        echo "  Suggested actions:"
        echo "  1. Switch model:  lumen -m deepseek-v4-pro"
        echo "  2. Check network: curl -I https://api.deepseek.com"
        echo "  3. Increase timeout: export LUMEN_API_TIMEOUT_SECS=60"
        echo "  4. Retry later: the API may be experiencing temporary issues"
        return 2
    elif [[ "$consecutive" -gt 0 ]]; then
        echo -e "${YELLOW}⚠${NC} API has ${consecutive} recent failures (below threshold ${THRESHOLD})"
        return 1
    fi

    echo -e "${GREEN}✓${NC} API healthy"
    return 0
}

cmd_watch() {
    echo "Watching $PROXY_LOG for API failures (Ctrl+C to stop)..."
    echo "Threshold: $THRESHOLD consecutive failures"
    echo ""

    if [[ ! -f "$PROXY_LOG" ]]; then
        touch "$PROXY_LOG"
    fi

    tail -n 0 -F "$PROXY_LOG" 2>/dev/null | while IFS= read -r line; do
        if echo "$line" | grep -q "context canceled\|upstream jitter.*retry"; then
            local count
            if command -v tac &>/dev/null; then
                count=$(tac "$PROXY_LOG" 2>/dev/null | head -20 | grep -c "context canceled\|upstream jitter" || echo "0")
            else
                count=$(tail -r "$PROXY_LOG" 2>/dev/null | head -20 | grep -c "context canceled\|upstream jitter" || echo "0")
            fi
            echo -e "${YELLOW}[$(date +%H:%M:%S)]${NC} API failure detected (${count} recent)"

            if [[ "$count" -ge "$THRESHOLD" ]]; then
                echo -e "${RED}[$(date +%H:%M:%S)] THRESHOLD EXCEEDED — consider switching model${NC}"
                echo "  → lumen -m deepseek-v4-pro"
            fi
        fi
    done
}

case "${1:-check}" in
    check)   cmd_check ;;
    --watch) cmd_watch ;;
    --reset) cmd_reset ;;
    *)       echo "Usage: $0 [check|--watch|--reset]"; exit 1 ;;
esac
