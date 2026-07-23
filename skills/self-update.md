# Lumen Self-Update Skill

当用户说"自己更新"、"优化自己"、"self-update"时，按以下流程执行。

## 流程

### Phase 1: 回顾记忆
读取 `~/.lumen/self-update-memory.json`。
- `applied_optimizations` 列表中的项目**已经应用过，不要再重复**
- 最多看一眼确认状态，不要重新修改已应用的优化
- 找出本次会话中遇到的新问题、新优化机会

### Phase 2: 分析改进点
回顾本次及最近会话中处理过的工作：
- 哪些 bug 修复值得固化？（如 lumen-guard 的 UTF-8 panic）
- 哪些配置调整效果显著？（如 DeepSeek cache 优化）
- 哪些代码改进能提升稳定性/性能？
- 排除掉 `applied_optimizations` 中已有的

### Phase 3: 实施修改
- 直接编辑源码：`/Users/lei/code/lumen/agent/crates/codegen/`
- 所有 lumen 自定义代码在 `lumen-guard/`、`lumen-discipline/`、`lumen-verify/`
- 品牌改动在 `xai-grok-pager/src/`、`xai-grok-pager-bin/src/`、`xai-grok-agent/src/`
- 模型配置在 `xai-grok-models/default_models.json`

### Phase 4: 测试
```bash
cd /Users/lei/code/lumen/agent && cargo test -p lumen-guard -p lumen-discipline -p lumen-verify 2>&1
```

### Phase 5: 提交
```bash
cd /Users/lei/code/lumen && git add -A && git commit -m "feat(self-update): <描述>"
```

### Phase 6: 编译安装重启
```bash
cd /Users/lei/code/lumen && ./scripts/self-update.sh
```

### Phase 7: 记录
将本次优化添加到 `~/.lumen/self-update-memory.json` 的 `applied_optimizations` 列表中：
```json
{
  "id": "<简短标识>",
  "description": "<做了什么>",
  "commit": "<git commit hash>",
  "timestamp": "<ISO时间>",
  "category": "bug-fix|performance|branding|config|security"
}
```

## 注意事项
- 不要改动 `agent/Cargo.toml` 或 `rust-toolchain.toml` 除非必要
- 不要删除 `lumen-guard`、`lumen-discipline`、`lumen-verify` 三个目录
- 编译后必须 `chmod 555` 保护二进制
- 如果 Grok Build 上游有更新，先用 review-upstream skill 审核
