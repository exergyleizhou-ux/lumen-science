# 01 — 注入地图（全部落到 Grok 真实路径）

> 路径相对 `~/code/lumen/agent/`。  
> **禁止**再写 claw 的 `crates/runtime/src/bash_validation.rs` 当目标（那是规格来源，不是落点）。

---

## A. 品牌 / 默认 / 去 xAI 化

| ID | 改动 | 精确落点 | 验收 |
|----|------|----------|------|
| A1 | 二进制名 `lumen` | `crates/codegen/xai-grok-pager-bin/Cargo.toml` → `[[bin]] name = "lumen"` | `./target/release/lumen --version` |
| A2 | 默认模型 DeepSeek | `crates/codegen/xai-grok-models/default_models.json` | 新会话默认 deepseek |
| A3 | 自定义模型文档落地 | 参考 `.../docs/user-guide/11-custom-models.md`；本机 `~/.grok` 或 `~/.lumen` config | 无浏览器 xAI 登录可 tool-use |
| A4 | 遥测默认关 | `crates/codegen/xai-grok-telemetry/` + 启动配置 | 无 mixpanel 外发 |
| A5 | auto_update 默认关 | `crates/codegen/xai-grok-update/` + config | 不请求 `https://x.ai/cli` |
| A6 | 用户可见文案 | `xai-grok-pager` welcome / help / version 串 | 显示 Lumen |
| A7 | 家目录迁移 | paths / config loader（优先兼容双读 `~/.grok`→`~/.lumen`） | 文档写清 |

---

## B. 安全注入（规格来自 Lumen + Claw 对照）

| ID | 能力 | 规格源 | **Grok 落点** | 验收 |
|----|------|--------|---------------|------|
| B1 | 5+1 层 bash 守卫 | `~/lumen/internal/guard/guard.go` | `crates/codegen/xai-grok-tools/src/implementations/grok_build/bash/mod.rs` **和/或** `crates/codegen/xai-grok-workspace/src/permission/shell_access.rs` + `rules.rs` | `scripts/smoke-security.sh` 全过 |
| B2 | 命令分段鉴权 | Claw bash_validation + Reasonix bash_decompose | `.../permission/bash_command_splitting.rs`（增强） | `a && rm -rf /` 只放过 a、拦 rm |
| B3 | 零宽字符剥离 | `guard.go` invisible chars | 新建薄模块 `.../util/` 或 hooks；**在 bash 执行前 + 可选 prompt 入口** | 含 ZWSP 的 `rm` 变体被拦 |
| B4 | write-path 黑名单 | `~/lumen/internal/guard/writepath.go` | `.../permission/rules.rs` + file write 工具路径检查 | 写 `~/.ssh/authorized_keys` 拒绝 |
| B5 | deny 优先于 bypass | Grok 已有语义 | 确认 `bypassPermissions` 仍尊重 deny | 文档+测试 |
| B6 | OS sandbox 推荐 | Grok sandbox | 默认文档/`--sandbox workspace` | doctor 提示 |

**对照源（只读）：**  
`claw-code-main/rust/crates/runtime/src/bash_validation.rs`  
→ 写入 `policy/CC_PARITY.md` 与 `policy/GUARD_RULES.md`，**不要**把文件拷进 claw 路径。

---

## C. Agent 纪律 / DeepSeek 脑

| ID | 能力 | 规格源 | Grok 落点 | 验收 |
|----|------|--------|-----------|------|
| C1 | cache 指标 UI | Reasonix /status | sampler 已有 cached tokens；**pager 状态栏/usage** | 长会话可见 cache |
| C2 | prefix 稳定纪律 | Reasonix REASONIX.md / cache_shape | prompt 组装审计 + `policy/REASONIX_PORT.md` | 文档+防回归检查 |
| C3 | compaction 调参 | Reasonix compact.go | `crates/common/xai-grok-compaction/` | 长会话不爆、策略可配 |
| C4 | Delivery / 假完成拦截 | Reasonix delivery_scope | agent 后置 / hooks PostToolUse / goal 工具 | 无证据 complete 被 nudge |
| C5 | Goal + todos 硬闸 | Reasonix + Grok UpdateGoalInput | **见 04§4.4**：`update_goal/mod.rs` gate + SessionActor Rejected；`todo/mod.rs` incomplete 查询 | DG3/DG4 测试 |
| C5b | Delivery turn-end | Reasonix delivery | **见 04§4.1–4.2**：agent turn 边界维护 DeliverySessionState；reminder 只进 turn tail | DG1/DG2 |
| C5c | Delivery hooks（备选） | Grok HookEventEnvelope | PreToolUse/`update_goal` Deny；PostToolUse 写 delivery.json | 可选 |
| C6 | Storm Breaker | Lumen agent.go | shell/agent 循环 **或** PostTool 钩子（先审计是否已有） | 3 次同错打断 |
| C7 | RepeatSuccessGuard | Lumen agent.go | 同上 | 3 次同成功警告 |
| C8 | Coordinator | Reasonix coordinator.go | `spawn_subagent` 模板（planner 只读） | 双 session 可演示 |
| C9 | editverify | `~/lumen/internal/editverify/*` | PostToolUse 批处理 或 内置 verify 步骤 | Go 项目改后 build/test 回灌 ≤3 轮 |

---

## D. Claw 反射（清单）

| ID | 动作 | 产出 |
|----|------|------|
| D1 | 写 `policy/CC_PARITY.md` ≥40 条 | 已有/部分/缺失/不做 |
| D2 | 缺失项补测试 | `evals/parity/` 或 crates 单测 |
| D3 | 改编 12 场景 harness | `scripts/parity-run.sh` |

---

## E. 垂直（Lumen packs）

| ID | 包 | 源 | 集成方式 | 验收 |
|----|-----|-----|----------|------|
| E1 | science | `~/lumen` science + cmds | `packs/science` 二进制 + MCP 注册进 grok | 3 步启动 |
| E2 | oasis | internal/oasis | `packs/oasis` CLI | `lumen oasis --help` |
| E3 | quant | internal/quant | 同上 | doctor 可探 |
| E4 | skills | `~/lumen/skills` | `~/.lumen/skills` 或 plugin dir | `/` 可调 skill |

---

## F. 明确不注入（v1）

| 项 | 原因 |
|----|------|
| Guardian 每工具 LLM 审查默认开 | 延迟/成本/误杀；v1.1 可选 |
| 完整 OTEL/Sentry 产品遥测 | 与「关遥测」冲突；可选本地 tracing |
| BM25 搜索引擎重写 | Grok 已有 codebase graph |
| 全量 crate 改名 xai→lumen | 单独里程碑 M-rename |
| IM Bot / 飞书 | 无关生产力主路径 |
| Memory v5 / 玄学 99K 行故事 | 不采信、不移植 |

---

## G. 改动纪律

1. **每个 ID 一个分支 / 一组 commit**，可回滚。  
2. 优先：**hooks + permission rules + config**，其次改 tools bash，最后才动 agent 循环。  
3. 改 pager 仅限：状态栏字段、welcome 文案、快捷配置入口——**禁止**重构 scrollback。  
4. 新增逻辑优先 **独立小模块文件**，避免 4000 行文件继续膨胀。  
