#!/usr/bin/env bash
# sync-upstream.sh — fetch Grok Build upstream and report what changed.
# Does NOT auto-merge (histories are unrelated). Prints a checklist for
# manual integration.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== 1/3 fetch upstream ==="
git fetch upstream --no-tags

echo ""
echo "=== 2/3 upstream latest commits ==="
git log --oneline upstream/main -10

echo ""
echo "=== 3/3 files changed in upstream vs last sync ==="
LAST_SYNC="$(git log --oneline --diff-filter=M -- agent/ | head -1 | awk '{print $1}')"
if [[ -z "$LAST_SYNC" ]]; then
  LAST_SYNC="853a305"  # Day 0 import
fi
echo "  last lumen sync commit: $LAST_SYNC"
echo "  upstream/main head:     $(git rev-parse --short upstream/main)"
echo ""
echo "  To see what changed in upstream since last check:"
echo "    git diff --stat $(git rev-parse --short upstream/main^^)..upstream/main"
echo ""
echo "  To port a specific upstream feature:"
echo "    1. git diff upstream/main^^..upstream/main -- crates/ > /tmp/upstream.patch"
echo "    2. Manually apply relevant hunks to agent/crates/ (paths differ: crates/ vs agent/crates/)"
echo "    3. Rebuild: ./scripts/install-local.sh"
echo ""
echo "=== upstream file layout (for path mapping) ==="
echo "  upstream: crates/codegen/xai-grok-pager/src/..."
echo "  ours:     agent/crates/codegen/xai-grok-pager/src/..."
echo ""
echo "=== our custom files (keep when updating) ==="
echo "  agent/crates/codegen/lumen-guard/"
echo "  agent/crates/codegen/lumen-discipline/"
echo "  agent/crates/codegen/lumen-verify/"
echo "  agent/crates/codegen/xai-grok-pager/assets/logo/logo05.txt"
echo "  agent/crates/codegen/xai-grok-pager/assets/logo/logo07.txt"
echo ""
echo "Done. Run 'git diff upstream/main^^..upstream/main' to inspect upstream changes."
