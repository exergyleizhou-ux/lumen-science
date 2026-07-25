#!/usr/bin/env bash
# lumen-config-check.sh — validate Lumen config consistency
#
# Checks for conflicts between ~/.lumen/config.toml (Lumen product config)
# and ~/.grok/config.toml (grok-build upstream config).
#
# Lumen uses TWO config files:
#   ~/.lumen/config.toml  — Lumen product config (FINAL-5UX). Primary home.
#   ~/.grok/config.toml   — grok-build upstream config. Override layer.
#
# Rules:
#   1. ~/.lumen/config.toml is the authoritative source for model defaults
#   2. ~/.grok/config.toml provides UI/runtime overrides (permissions, auto_update)
#   3. If both define [models].default, ~/.lumen wins
#   4. Model definitions with same key in both → ~/.lumen wins
#
# Usage:
#   ./scripts/lumen-config-check.sh

set -euo pipefail

LUMEN_CONFIG="$HOME/.lumen/config.toml"
GROK_CONFIG="$HOME/.grok/config.toml"

RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
NC='\033[0m'

echo "=== Lumen Config Check ==="
echo ""

issues=0

# Check files exist
for f in "$LUMEN_CONFIG" "$GROK_CONFIG"; do
    if [[ ! -f "$f" ]]; then
        echo -e "${RED}✗${NC} Missing: $f"
        issues=$((issues + 1))
    fi
done

if [[ $issues -gt 0 ]]; then
    echo ""
    echo "Fix: create the missing config file(s)"
    exit 1
fi

echo -e "${GREEN}✓${NC} Both config files exist"
echo ""

# Check for conflicting model defaults
echo "--- Model defaults ---"
lumen_default=$(grep "^default\s*=" "$LUMEN_CONFIG" 2>/dev/null | head -1 | sed 's/.*=\s*"\(.*\)".*/\1/' || echo "")
grok_default=$(grep "^default\s*=" "$GROK_CONFIG" 2>/dev/null | head -1 | sed 's/.*=\s*"\(.*\)".*/\1/' || echo "")

if [[ -n "$lumen_default" ]] && [[ -n "$grok_default" ]]; then
    if [[ "$lumen_default" != "$grok_default" ]]; then
        echo -e "${YELLOW}⚠${NC}  Default model mismatch:"
        echo "   ~/.lumen:  $lumen_default (authoritative)"
        echo "   ~/.grok:   $grok_default (overridden)"
        echo "   → Using $lumen_default from ~/.lumen/config.toml"
        issues=$((issues + 1))
    else
        echo -e "${GREEN}✓${NC}  Default model: $lumen_default (consistent)"
    fi
else
    echo -e "${YELLOW}⚠${NC}  Could not parse default model from one or both configs"
fi

# Check for duplicate model definitions
echo ""
echo "--- Model definitions ---"
lumen_models=$(grep "^\[model\." "$LUMEN_CONFIG" 2>/dev/null | sed 's/\[model\.\(.*\)\]/\1/' || echo "")
grok_models=$(grep "^\[model\." "$GROK_CONFIG" 2>/dev/null | sed 's/\[model\.\(.*\)\]/\1/' || echo "")

duplicates=$(comm -12 <(echo "$lumen_models" | sort) <(echo "$grok_models" | sort) 2>/dev/null || echo "")
if [[ -n "$duplicates" ]]; then
    echo -e "${YELLOW}⚠${NC}  Models defined in both configs (Lumen wins):"
    echo "$duplicates" | while read -r model; do
        echo "   - $model"
    done
else
    echo -e "${GREEN}✓${NC}  No duplicate model definitions"
fi

# Check auto_update consistency
echo ""
echo "--- Update settings ---"
lumen_update=$(grep "auto_update" "$LUMEN_CONFIG" 2>/dev/null || echo "not set")
grok_update=$(grep "auto_update" "$GROK_CONFIG" 2>/dev/null || echo "not set")
echo "  ~/.grok: $grok_update (should be false for Lumen fork)"

echo ""
if [[ $issues -eq 0 ]]; then
    echo -e "${GREEN}✓ Config check passed — no issues found${NC}"
else
    echo -e "${YELLOW}⚠ Found $issues potential issue(s)${NC}"
fi

exit 0
