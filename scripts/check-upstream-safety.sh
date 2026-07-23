#!/usr/bin/env bash
# check-upstream-safety.sh — 硬性安全检查上游变更
# 不依赖 AI，纯机械检查。返回非零 = 有风险，需人工介入。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "══ 上游安全硬检查 ══"
echo ""

# ── 1. Fetch ──
git fetch upstream --no-tags 2>/dev/null || true
UP_HEAD=$(git rev-parse upstream/main)

# ── 2. 保护区清单（绝对不允许上游触及）──
PROTECTED=(
    "agent/crates/codegen/lumen-guard"
    "agent/crates/codegen/lumen-discipline"
    "agent/crates/codegen/lumen-verify"
    "agent/crates/codegen/xai-grok-pager/assets/logo/logo05.txt"
    "agent/crates/codegen/xai-grok-pager/assets/logo/logo07.txt"
    "agent/crates/codegen/xai-grok-agent/src/prompt/context.rs"
    "agent/crates/codegen/xai-grok-pager/src/views/welcome/hero_box.rs"
    "agent/crates/codegen/xai-grok-pager/src/views/welcome/mod.rs"
)

# ── 3. 我们修改过的文件（有冲突风险）──
OUR_FILES=$(git diff --name-only 853a305..HEAD 2>/dev/null | sort)

# ── 4. 上游变更文件 ──
UP_FILES_RAW=$(git diff --name-only "$UP_HEAD"^^.."$UP_HEAD" 2>/dev/null || echo "")
UP_FILES=$(echo "$UP_FILES_RAW" | sort)

# ── 5. 保护区检查 ──
echo "--- 保护区检查 ---"
PROTECTED_HIT=0
for p in "${PROTECTED[@]}"; do
    if echo "$UP_FILES_RAW" | grep -q "$p"; then
        echo "  🛡️ ALERT: 上游触及保护区: $p"
        PROTECTED_HIT=1
    fi
done
if [[ $PROTECTED_HIT -eq 0 ]]; then
    echo "  ✅ 保护区安全"
fi

# ── 6. 冲突检测（路径映射后）──
echo ""
echo "--- 冲突检测 ---"
CONFLICTS=0
while IFS= read -r up_file; do
    [[ -z "$up_file" ]] && continue
    # 上游路径映射到我们的路径
    our_file="${up_file/crates\//agent\/crates\/}"
    if echo "$OUR_FILES" | grep -qF "$our_file"; then
        echo "  🔴 CONFLICT: $our_file"
        CONFLICTS=$((CONFLICTS + 1))
    fi
done <<< "$UP_FILES"

if [[ $CONFLICTS -eq 0 ]]; then
    echo "  ✅ 无冲突"
else
    echo "  ⚠️  $CONFLICTS 个文件有冲突风险"
fi

# ── 7. 品牌/市场文件过滤 ──
echo ""
echo "--- 品牌/市场/XAI 相关 ---"
SKIP_COUNT=$(echo "$UP_FILES_RAW" | grep -cE 'docs/|\.md$|marketplace|billing|upgrade|subscription|grok\.com|x\.ai|README|CHANGELOG' || echo 0)
echo "  ⚫ $SKIP_COUNT 个文件建议跳过（品牌/市场/文档）"

# ── 8. 汇总 ──
echo ""
echo "--- 总结 ---"
TOTAL=$(echo "$UP_FILES_RAW" | grep -c . || echo 0)
echo "  上游变更总数: $TOTAL"
echo "  冲突: $CONFLICTS"
echo "  保护区告警: $PROTECTED_HIT"
echo "  建议跳过: $SKIP_COUNT"
echo "  可吸收: $((TOTAL - CONFLICTS - PROTECTED_HIT - SKIP_COUNT))"

if [[ $PROTECTED_HIT -gt 0 ]]; then
    echo ""
    echo "  ❌ 硬阻止：上游触及保护区，禁止自动合并！"
    exit 1
elif [[ $CONFLICTS -gt 0 ]]; then
    echo ""
    echo "  ⚠️  有冲突需人工处理"
    exit 2
else
    echo ""
    echo "  ✅ 安全：可以进一步审核"
fi
