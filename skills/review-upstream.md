# Lumen Upstream Review Skill

当用户说"检查上游更新"、"审核 Grok Build"、"review upstream"时执行。

## 核心原则
- **绝不自动合并**——生成审核报告，等用户审批
- **硬检查优先**——先跑 `scripts/check-upstream-safety.sh` 做机械验证
- **AI 做语义分析，脚本做硬约束，人做最终决定**
- **保护我们的三层（guard/discipline/verify）**
- **保护品牌文件（logo/welcome/label）**
- **路径映射**：上游 `crates/codegen/xxx` → 我们 `agent/crates/codegen/xxx`

## 执行流程

### Phase 1: 硬安全检查（脚本，不可跳过）
```bash
cd /Users/lei/code/lumen && ./scripts/check-upstream-safety.sh
```
- 退出码 0 = 安全，继续
- 退出码 1 = 保护区告警，**硬阻止，立即报告用户，停止**
- 退出码 2 = 有冲突，标记后继续

### Phase 2: AI 语义分析（逐文件 diff 阅读）
```bash
cd /Users/lei/code/lumen
git fetch upstream --no-tags
echo "=== 上游新提交 ==="
git log upstream/main -5 --oneline --no-decorate
echo "=== 变更统计 ==="
git diff --stat upstream/main^^..upstream/main
```

### Phase 2: 逐文件分类
对上游变更的每个文件，打上标签：

| 标签 | 含义 |
|------|------|
| 🟢 SAFE | 新功能，不冲突，可直接吸收 |
| 🟡 REVIEW | 功能相关但需人工看 |
| 🔴 CONFLICT | 与我们的改动冲突 |
| ⚫ SKIP | xAI品牌/市场/文档，跳过 |
| 🛡️ PROTECTED | 我们的保护区，永不覆盖 |

### Phase 3: 生成审核报告
输出结构化报告给用户：

```
═══════════════════════════════════
  Grok Build 上游审核报告
  上游 commit: <hash>
  变更文件: N 个
═══════════════════════════════════

🟢 可安全吸收 (X 个):
  - crates/xxx → agent/crates/xxx  原因：新工具实现

🟡 需审核 (X 个):
  - crates/yyy → agent/crates/yyy  原因：涉及 tool dispatch

🔴 冲突 (X 个):
  - crates/zzz → agent/crates/zzz  原因：我们改了同一函数

⚫ 跳过 (X 个):
  - docs/xxx  原因：Grok 品牌文档

🛡️ 保护区 (不受影响):
  - lumen-guard / lumen-discipline / lumen-verify
  - logo05.txt / logo07.txt / hero_box.rs / context.rs
═══════════════════════════════════
建议操作：[逐个列出，等待用户审批]
═══════════════════════════════════
```

### Phase 4: 等待用户审批
用户会回复"吸收 X, Y"或"全部跳过"或"只吸收安全的"。

### Phase 5: 执行吸收
对每个被批准的 🟢 文件：
1. 读取上游文件内容
2. 找到对应我们文件的位置（路径映射：`crates/` → `agent/crates/`）
3. 手动 patch 变更到我们文件
4. 确认保护区文件未被修改

对 🟡 文件：分析具体变更，只吸收非冲突部分。

### Phase 6: 验证 & 记录
```bash
cd /Users/lei/code/lumen/agent && cargo test -p lumen-guard -p lumen-discipline -p lumen-verify
cd /Users/lei/code/lumen && git diff --stat
```
一切正常后：
```bash
cd /Users/lei/code/lumen && git add -A && git commit -m "feat(upstream): absorb <具体描述>"
```
更新 `~/.lumen/self-update-memory.json` 的 `upstream_reviews`。

## 注意事项
- 每次只处理一个上游版本
- 历史不连通，不要用 git merge
- 手动 patch 而非自动合并
- 编译前先检查 `git status` 是否干净
- 如果用户不想执行，只生成报告并退出
