# Lumen 分工计划 — 我和 Codex 并行

> 目标：所有改动最终合入 `main`，不冲突，可编译，测试通过。

---

## 当前状态

```
main (a74305f) ← 我的最新，含 storm/delivery wired + 测试修复
  ├── codex/production-ready-p1-rust (febb833) — 基于旧 base b156143
  │   ├── truth hardening (24 files, +890/-108)
  │   └── 删除了 storm/delivery 字段（因为在旧 base 上不存在）
  │
  └── codex/enhancements-f-release (f415354) — 基于 main 的 b933176
      ├── 发布流水线基础设施 (6 files, +2482)
      └── 包含测试修复（与我的 a74305f 重复）
```

### 关键冲突
P1 分支删除了 `storm_breaker`/`repeat_success_guard`/`delivery_state` 三个字段（因为在它的 base b156143 上不存在）。但 main 已经有这些字段（我的 b933176 加的）。所以 P1 直接合入 main 会：
- 尝试删除不存在的字段 → 编译错误
- 需要 rebase 到 main 并解决冲突

---

## 分工

### 我（当前会话）— 任务 A: 合入 P1 + 修复冲突

**目标**：将 `codex/production-ready-p1-rust` 的 truth hardening 改动合入 main，保留 storm/delivery 字段。

**步骤**：
1. 在 P1 分支上 `git rebase main`，解决冲突
   - 冲突在 `acp_session.rs`、`spawn.rs`、`tool_calls.rs`、`turn.rs` 等文件
   - P1 删除了 storm/delivery 字段 → 保留 main 的版本（不删除）
   - P1 的其他改动（truth hardening）全部保留
2. 编译验证 `cargo check -p xai-grok-shell --lib`
3. 运行测试 `cargo test -p xai-grok-shell goal_planner`
4. 合入 main

### Codex 会话1（production-ready-p1-rust）— 任务 B: 合入 F + 补发布流水线缺口

**目标**：将 `codex/enhancements-f-release` 的发布基础设施合入 main，补齐审计发现的缺口。

**步骤**：
1. 等我把 P1 合入 main 后，在 F 分支上 `git rebase main`
2. 解决与我的测试修复的冲突（`cancel_running_task_tests.rs`、`inline_auto_compact_flow_tests.rs`）
3. 补齐审计发现的 F 缺口：
   - 创建 `scripts/release.sh`（F 分支没有这个文件）
   - 实现 version bump 机制
   - 实现 CHANGELOG 自动更新
   - 配置 GitHub remote（当前只有 upstream，没有 Lumen 自有发布 remote）
4. 编译验证 + 运行 `scripts/test-release-contract.sh`
5. 合入 main

### Codex 会话2（enhancements-f-release）— 任务 C: 修复版本矛盾 + 沙箱网络隔离

**目标**：修复审计发现的 2 个具体 bug。

**步骤**：
1. 基于 main 创建新分支 `codex/fix-audit-issues`
2. 修复版本矛盾：
   - 文件: `agent/crates/codegen/xai-grok-update/src/auto_update.rs`
   - 问题: `lumen update --check --json` 报告 `currentVersion=0.2.0-dev`，但 `lumen --version` 是 `0.1.220-alpha.4`
   - 原因: 上游 Grok Build 的版本检测逻辑与 Lumen 自建版本号不一致
   - 修复: 让 updater 读取 `lumen --version` 的真实版本号，而不是硬编码的 `0.2.0-dev`
3. 修复 macOS 沙箱网络隔离：
   - 文件: `agent/crates/codegen/xai-grok-sandbox/src/child_net.rs:108-110`
   - 问题: macOS 路径直接 `Ok(())`，没有实际网络隔离
   - 修复: 实现 macOS 的 `sandbox-exec` 网络限制 profile
4. 编译验证 + 运行沙箱测试
5. 合入 main

---

## 时间线

```
我（现在）: 合入 P1 → main
                ↓
Codex 会话1: 合入 F → main + 补发布缺口
                ↓
Codex 会话2: 修复版本矛盾 + 沙箱网络隔离
                ↓
所有改动在 main，测试通过
```

## 文件冲突地图

| 文件 | P1 改动 | F 改动 | 我的改动 | 策略 |
|------|---------|--------|---------|------|
| `acp_session.rs` | 删 3 字段 | 无 | 加 3 字段 | 保留 main |
| `spawn.rs` | 删 3 行 | 无 | 加 3 行 | 保留 main |
| `tool_calls.rs` | 删 storm/delivery wiring | 无 | 加 storm/delivery wiring | 保留 main |
| `turn.rs` | 删 delivery turn-end | 无 | 加 delivery turn-end | 保留 main |
| `cancel_running_task_tests.rs` | 无 | 加字段 | 加字段 | 合并（内容相同） |
| `inline_auto_compact_flow_tests.rs` | 无 | 加字段 | 加字段 | 合并（内容相同） |
| `ENHANCEMENTS.md` | 无 | 加内容 | 加内容 | 取最新版本 |
