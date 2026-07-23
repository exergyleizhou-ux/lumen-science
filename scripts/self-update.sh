#!/usr/bin/env bash
# self-update.sh — lumen 自更新：编译 → 安装 → 重启
# 由 lumen AI 调用，或在终端手动执行。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"

MEMORY_FILE="$HOME/.lumen/self-update-memory.json"

echo "══ lumen self-update ══"
echo "source: $ROOT"
echo ""

# ── 1. 编译 ──
echo "═══ 1/4 编译 ═══"
cd "$ROOT"
if [[ -n "$(git -C "$ROOT" status --porcelain --untracked-files=all)" ]]; then
    echo "FAIL: self-update refuses a dirty source tree." >&2
    echo "Commit and review the optimization layer before building or installing it." >&2
    git -C "$ROOT" status --short >&2
    exit 1
fi
unset LUMEN_ALLOW_DIRTY
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export LUMEN_SKIP_BUILD=0
./scripts/install-local.sh
echo ""

# ── 2. 安装到 leader 位置 ──
echo "═══ 2/4 安装 leader 二进制 ═══"
LEADER_DST="$HOME/.grok/downloads/grok-0.2.102-macos-aarch64"
if [[ -f "$LEADER_DST" ]]; then
    chmod 755 "$LEADER_DST" 2>/dev/null || true
fi
cp "$HOME/.local/bin/lumen" "$LEADER_DST"
# macOS code-signing: replace with ad-hoc signature so taskgated doesn't kill us
codesign --force --sign - "$LEADER_DST" 2>/dev/null || true
chmod 555 "$LEADER_DST"
echo "  installed: $LEADER_DST"
echo "  version:   $($LEADER_DST --version 2>&1)"
echo ""

# ── 3. 记录更新记忆 ──
echo "═══ 3/4 记录更新 ═══"
mkdir -p "$(dirname "$MEMORY_FILE")"
COMMIT=$(git -C "$ROOT" rev-parse --short HEAD)
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)

if [[ -f "$MEMORY_FILE" ]]; then
    # append to existing memory
    python3 -c "
import json, sys
with open('$MEMORY_FILE') as f:
    mem = json.load(f)
mem.setdefault('updates', []).append({
    'commit': '$COMMIT',
    'timestamp': '$TIMESTAMP',
    'source': 'self-update.sh'
})
with open('$MEMORY_FILE', 'w') as f:
    json.dump(mem, f, indent=2, ensure_ascii=False)
print('  appended to memory: $COMMIT')
" 2>/dev/null || echo "  (memory update skipped)"
else
    cat > "$MEMORY_FILE" << JSONEOF
{
  "updates": [
    {"commit": "$COMMIT", "timestamp": "$TIMESTAMP", "source": "self-update.sh"}
  ],
  "applied_optimizations": [],
  "upstream_reviews": []
}
JSONEOF
    echo "  created memory: $MEMORY_FILE"
fi
echo ""

# ── 4. 重启 ──
echo "═══ 4/4 重启 lumen ═══"
echo "  杀掉旧进程..."
pkill -f "grok-0.2.102" 2>/dev/null || true
pkill -f "lumen" 2>/dev/null || true
sleep 0.5

echo ""
echo "══ 自更新完成 ══"
echo "  新版本: $($LEADER_DST --version 2>&1)"
echo "  下一步: 在终端重新运行 lumen"
echo ""
