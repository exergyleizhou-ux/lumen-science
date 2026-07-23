# Lumen 增强规格书 — 给 Codex 的执行指令

> 本文档写给 Codex agent，让它能独立完成每个增强任务。
> 每个任务包含：目标、参考实现、文件位置、验收标准。

## 当前状态（2026-07-18）— 更新于 P1 合入后

审计结论：**DONE_WITH_CONCERNS**。29 条验收标准中严格完成 7 条（24.14%）。

| Phase | DONE | CHANGED | PARTIAL | NOT DONE | UNVERIFIABLE | 严格完成 |
|:-----|:---:|:-------:|:-------:|:--------:|:-----------:|:--------:|
| A — 自动更新 | 0 | 0 | 4 | 0 | 0 | 0/4 |
| B — MCP 插件 | 4 | 0 | 1 | 0 | 0 | 4/5 |
| C — 多模型协作 | 0 | 1 | 2 | 1 | 0 | 1/4 |
| D — 桌面应用 | 0 | 0 | 0 | 4 | 0 | 0/4 |
| E — IM 机器人 | 0 | 0 | 0 | 4 | 0 | 0/4 |
| F — 发布流水线 | 0 | 0 | 2 | 2 | 0 | 0/4 |
| G — 沙箱 | 2 | 0 | 0 | 1 | 1 | 2/4 |
| **合计** | **6** | **1** | **9** | **12** | **1** | **7/29** |

### 分支状态

| 分支 | commit | 状态 | 说明 |
|------|--------|------|------|
| `main` | `877ecbd` | ✅ 当前运行 | P1 truth hardening 已合入 + StormBreaker/Delivery 保留 |
| `codex/production-ready-p1-rust` | `febb833` | ✅ 已合入 main | truth hardening 通过 patch 方式合入（排除冲突部分） |
| `codex/enhancements-f-release` | `f415354` | ✅ 已有 6 files +2482 实现 | 发布流水线基础设施，待合入 |

### 已修复的问题
1. ~~测试编译失败~~ ✅ 已修复（commit `a74305f`）：3 个测试文件补了 `delivery_state`/`repeat_success_guard`/`storm_breaker` 字段
2. ~~P1 未合入~~ ✅ 已合入（commit `877ecbd`）：truth hardening 24 文件 +816/-108，编译通过，测试 76/76

### 已知问题（待 Codex 修复）
1. `lumen update --check --json` 报告 `currentVersion=0.2.0-dev` 与 `lumen --version` 的 `0.1.220-alpha.4` 矛盾
2. macOS 沙箱网络隔离是 no-op（`xai-grok-sandbox/src/child_net.rs:108-110`）
3. 无 Lumen 自有发布 remote，唯一 remote 是 `xai-org/grok-build`
4. F 分支发布基础设施未合入，缺 `scripts/release.sh`、version bump、CHANGELOG 自动更新
2. `lumen update --check --json` 报告 `currentVersion=0.2.0-dev` 与 `lumen --version` 的 `0.1.220-alpha.4` 矛盾
3. macOS 沙箱网络隔离是 no-op（`xai-grok-sandbox/src/child_net.rs:108-110`）
4. 无 Lumen 自有发布 remote，唯一 remote 是 `xai-org/grok-build`

### 建议合入顺序
1. 修复 main 中 3 个过期 `SessionActor` 测试构造器（补 `delivery_state`/`repeat_success_guard`/`storm_breaker`）
2. 将 P1 (`febb833`) 重放到当前 main（不是 cherry-pick `b933176`——它已在 main）
3. 在 P1 集成后的 main 上合入 F release foundation
4. 补 `scripts/release.sh`、version bump、CHANGELOG 自动更新
5. 让 A updater 消费 F 的 Lumen 签名 manifest
6. 完成 C 的双模型 + 审批流
7. 完成 B 的插件专属沙箱
8. 修复 G 的 macOS 网络隔离
9. 最后实施 D/E

---

## 任务 A: 自动更新系统

### 目标
让 Lumen 支持 `lumen upgrade` 命令，自动检查 GitHub Releases 并更新二进制。

### 参考实现
**DeepSeek-Reasonix** 的自动更新系统：
- GitHub: https://github.com/esengine/DeepSeek-Reasonix
- 核心文件: `desktop/updater.go` — 多级 manifest 获取 + minisign 签名验证 + 平台适配
- 架构: 3 级 fallback (R2 CDN → Crash worker → GitHub Releases) + 签名验证 + 原子替换

### 当前 Lumen 位置
- 二进制: `~/.local/bin/lumen`（安装目标）
- 编译脚本: `scripts/install-local.sh`（当前手动编译安装）
- 版本信息: `lumen --version` 输出 `0.1.220-alpha.4 (b933176)`
- 上游仓库: https://github.com/xai-org/grok-build

### 具体实现

#### A1. 创建 `scripts/upgrade.sh`
参考 `scripts/install-local.sh` 的编译逻辑，新增：
- 检查 GitHub Releases 获取最新版本号
- 对比本地版本 (`lumen --version` 解析)
- 如果有新版本：`git fetch upstream && git merge` → `cargo build --release` → 原子替换二进制
- 失败回滚（保留旧二进制备份）

#### A2. 创建 slash command `/upgrade`
文件: `agent/crates/codegen/xai-grok-pager/src/slash/commands/`
参考已有命令（如 `/compact`）的注册方式，新增 `upgrade.rs`：
- 调用 `scripts/upgrade.sh`
- 显示更新进度
- 提示重启 session

#### A3. 构建时嵌入版本信息
文件: `agent/crates/codegen/xai-grok-pager-bin/build.rs`
参考 Claw Code 的 `build.rs` 做法：
- 嵌入 git SHA、构建时间、rustc 版本
- 在 `lumen --version` 中显示完整 provenance

### 验收标准
- [ ] `lumen upgrade` 检查更新并报告当前/最新版本
- [ ] 有更新时自动拉取、编译、安装
- [ ] 失败时回滚到旧版本
- [ ] `lumen --version --json` 输出完整 provenance

---

## 任务 B: MCP 插件系统

### 目标
让 Lumen 支持 MCP (Model Context Protocol) 插件，可以动态加载外部工具服务器。

### 参考实现
**Claw Code** 的 MCP 插件系统：
- GitHub: https://github.com/ultraworkers/claw-code
- 插件管理: `rust/crates/plugins/src/lib.rs` (3,863 LOC) — 安装/启用/禁用/更新/卸载
- 插件钩子: `rust/crates/plugins/src/hooks.rs` (564 LOC) — PreToolUse/PostToolUse
- 插件生命周期: `rust/crates/runtime/src/plugin_lifecycle.rs` (592 LOC) — 11 阶段状态机
- 沙箱: `rust/crates/runtime/src/sandbox.rs` (384 LOC) — Linux namespace 隔离

### 当前 Lumen 位置
- 上游 Grok Build 已有基础 MCP 支持（`xai-grok-shell/src/session/mcp_servers/`）
- 但缺少插件管理（安装/卸载/更新）、插件市场、沙箱隔离

### 具体实现

#### B1. 插件注册表
新建文件: `agent/crates/codegen/lumen-plugin/`
参考 Claw Code 的 `plugins/src/lib.rs`：
- `PluginManifest` 结构体（name, version, description, tools, hooks）
- `PluginRegistry` — 扫描 `~/.lumen/plugins/` 和项目 `.lumen/plugins/`
- 安装: 从 URL 或本地路径复制到插件目录
- 启用/禁用: 修改 `config.toml` 的 `[plugins]` 段

#### B2. 插件生命周期管理
新建文件: `agent/crates/codegen/lumen-plugin/src/lifecycle.rs`
参考 Claw Code 的 `runtime/src/plugin_lifecycle.rs`：
- 状态机: Discovered → Resolved → Verified → Launched → Ready → Stopped
- 超时控制: 每个阶段有超时，超时则标记为 Failed
- 健康检查: 定期 ping 插件进程

#### B3. 插件沙箱
新建文件: `agent/crates/codegen/lumen-plugin/src/sandbox.rs`
参考 Claw Code 的 `runtime/src/sandbox.rs`：
- macOS: `sandbox-exec` (Seatbelt)
- Linux: `bubblewrap` 或 user namespace
- 配置: 读/写路径白名单、网络访问控制

#### B4. 插件市场 slash commands
文件: `agent/crates/codegen/xai-grok-pager/src/slash/commands/`
新增命令:
- `/plugin install <url>` — 安装插件
- `/plugin list` — 列出已安装插件
- `/plugin remove <name>` — 卸载插件
- `/plugin update <name>` — 更新插件

### 验收标准
- [ ] 可以从 URL 安装 MCP 插件
- [ ] 插件工具自动注册到工具列表
- [ ] 插件在沙箱中运行（路径隔离）
- [ ] `/plugin list` 显示所有插件状态
- [ ] 插件崩溃后自动重启（最多 3 次）

---

## 任务 C: 多模型协作（Planner + Executor）

### 目标
支持双模型协作：一个模型负责规划（Planner），另一个负责执行（Executor），各自有独立的 cache-stable session。

### 参考实现
**DeepSeek-Reasonix** 的 Coordinator：
- GitHub: https://github.com/esengine/DeepSeek-Reasonix
- 核心文件: `internal/agent/coordinator.go` (914 LOC)
- 架构: `Coordinator` 结构体包含 `planner provider.Provider` + `executor *Agent`
- Planner 输出计划 → Executor 逐步执行
- 计划审批: `PlannerPlanApprover` 接口让用户可以审批/拒绝计划
- 配置: `planner_model = "deepseek-pro"` 在 `reasonix.example.toml` 中

### 当前 Lumen 位置
- 已有 `fork_secondary_model = "deepseek-reasoner"` 在 `~/.grok/config.toml`
- 已有 `/fork` 命令（手动分支）
- 缺少自动双模型协作

### 具体实现

#### C1. Coordinator 结构体
新建文件: `agent/crates/codegen/lumen-discipline/src/coordinator.rs`
参考 Reasonix 的 `coordinator.go`：
```rust
pub struct Coordinator {
    planner: ModelProvider,    // 规划模型（如 deepseek-reasoner）
    executor: ModelProvider,   // 执行模型（如 deepseek-chat）
    plan: Option<String>,      // 当前计划
    step_index: usize,         // 执行到第几步
}
```

#### C2. Planner Prompt
参考 Reasonix 的 `DefaultPlannerPrompt`：
```
You are the planner in a two-model coding agent.
Produce concise, step-by-step plans. Do NOT execute anything.
Each step must be concrete and actionable.
```

#### C3. 配置集成
文件: `~/.grok/config.toml`
新增配置段:
```toml
[coordinator]
enabled = false
planner_model = "deepseek-reasoner"
executor_model = "deepseek-chat"
auto_plan = true
```

#### C4. Slash 命令
新增 `/plan-auto` 切换自动规划模式，`/plan-review` 查看当前计划。

### 验收标准
- [ ] 启用 coordinator 后，复杂任务先出计划再执行
- [ ] Planner 和 Executor 使用不同模型
- [ ] 用户可以在执行前审批/修改计划
- [ ] 各自的 session 保持 cache 稳定

---

## 任务 D: 桌面应用

### 目标
为 Lumen 构建一个可选的桌面应用（macOS 优先），使用 Wails 或 Tauri。

### 参考实现
**DeepSeek-Reasonix** 的桌面应用：
- GitHub: https://github.com/esengine/DeepSeek-Reasonix
- 入口: `desktop/main.go` — Wails shell
- 前端: `desktop/frontend/` — React + TypeScript + Vite (~260 文件)
- 架构: 嵌套 Go module，不污染 CLI 的 `CGO_ENABLED=0`

**旧 Lumen (Go)** 的桌面应用：
- GitHub: https://github.com/exergyleizhou-ux/lumen
- 目录: `desktop/` — Tauri 应用
- Science Lab + Science 两个桌面应用

### 当前 Lumen 位置
- 无桌面应用
- 当前是纯 CLI TUI（基于 Grok Build 的 pager）

### 具体实现

#### D1. 桌面 shell
新建目录: `desktop/`
参考 Reasonix 的 `desktop/main.go`：
- 使用 Wails v2（Go 生态，与 Lumen 的 Rust 主体不同——需要额外进程通信）
- 或者使用 Tauri（Rust 生态，可以直接链接 Lumen 的 Rust crate）

推荐 Tauri 方案（Rust 生态，可以直接调用 Lumen 的 Rust API）：
- `desktop/src-tauri/` — Rust 后端
- `desktop/src/` — React/TypeScript 前端

#### D2. ACP 通信
Lumen 已有 ACP (Agent Client Protocol) 支持（`xai-grok-shell/src/session/acp_session.rs`）。
桌面应用通过 ACP WebSocket 或 stdio 与 Lumen 内核通信。

#### D3. 前端功能
- 会话列表/管理
- 模型切换
- 插件管理 UI
- 设置面板
- 主题切换

### 验收标准
- [ ] 桌面应用可以启动并连接到 Lumen 内核
- [ ] 基本的聊天界面可用
- [ ] 模型切换、会话管理可用
- [ ] macOS 原生菜单和快捷键

---

## 任务 E: IM 机器人网关

### 目标
让 Lumen 可以通过 QQ、飞书、微信等 IM 平台交互。

### 参考实现
**DeepSeek-Reasonix** 的 Bot 系统：
- GitHub: https://github.com/esengine/DeepSeek-Reasonix
- QQ: `internal/bot/qq/adapter.go` + `gateway.go`
- 飞书: `internal/bot/feishu/feishu.go` + `inbound.go` + `outbound.go`
- 微信: `internal/bot/weixin/weixin.go` + `weixin_login.go`
- 通用: `internal/bot/gateway.go` — 统一消息路由
- 桌面集成: `desktop/bot_bridge.go` — 桌面端显示 bot 会话

### 当前 Lumen 位置
- 无 IM 集成
- 纯 CLI/TUI

### 具体实现

#### E1. Bot 网关抽象
新建目录: `agent/crates/codegen/lumen-bot/`
```rust
pub trait BotAdapter {
    async fn send_message(&self, msg: &str) -> Result<()>;
    async fn handle_message(&self, msg: &str) -> Result<String>;
}
```

#### E2. QQ 适配器
参考 Reasonix 的 `internal/bot/qq/`：
- 使用 QQ 官方 API 或 OneBot 协议
- 消息收发 + 图片支持

#### E3. 飞书适配器
参考 Reasonix 的 `internal/bot/feishu/`：
- 飞书开放 API
- 消息卡片支持

#### E4. 配置
```toml
[bot]
enabled = false

[bot.qq]
enabled = false
api_url = "..."
api_key = "..."

[bot.feishu]
enabled = false
app_id = "..."
app_secret = "..."
```

### 验收标准
- [ ] 至少一个 IM 平台可以收发消息
- [ ] 支持文本对话
- [ ] 支持代码块渲染
- [ ] 支持文件/图片收发

---

## 任务 F: 发布流水线

### 目标
建立自动化的发布流水线：版本 bump → 编译 → 签名 → 发布 GitHub Release → 通知更新。

### 参考实现
**DeepSeek-Reasonix** 的发布系统：
- GitHub: https://github.com/esengine/DeepSeek-Reasonix
- 发布脚本: `scripts/resolve-desktop-release.sh`, `resolve-npm-release.sh`, `resolve-stable-release.sh`
- 验证脚本: `scripts/verify-release-authorization.sh`, `verify-release-tag.sh`, `verify-stable-release-artifacts.sh`
- 发布说明: `scripts/generate-release-notes.mjs`
- 发布渠道: R2 CDN → Crash worker → GitHub Releases → npm

**旧 Lumen (Go)** 的发布：
- GitHub: https://github.com/exergyleizhou-ux/lumen
- `install.sh` — 从 GitHub Releases 下载或源码编译
- goreleaser — 自动构建跨平台二进制

### 当前 Lumen 位置
- 无发布流水线
- 只有手动 `scripts/install-local.sh`
- 无 GitHub 远程仓库

### 具体实现

#### F1. GitHub Actions 工作流
新建文件: `.github/workflows/release.yml`
```yaml
name: Release
on:
  push:
    tags: ['v*']
jobs:
  build:
    strategy:
      matrix:
        os: [macos-latest, ubuntu-latest]
    steps:
      - uses: actions/checkout@v4
      - run: cargo build --release
      - run: scripts/codesign.sh  # macOS 签名
      - uses: softprops/action-gh-release@v2
        with:
          files: agent/target/release/lumen
```

#### F2. 版本管理
新建文件: `VERSION`（当前 `0.1.220-alpha.4`）
参考语义化版本：`MAJOR.MINOR.PATCH` + 预发布标签

#### F3. 发布脚本
新建文件: `scripts/release.sh`
- 验证 git 状态干净
- 版本 bump（`VERSION` 文件 + git tag）
- 编译所有目标平台
- 生成 checksum
- 创建 GitHub Release
- 更新 `CHANGELOG.md`

#### F4. 多平台编译
修改 `scripts/install-local.sh` 支持交叉编译：
- macOS (arm64, amd64)
- Linux (arm64, amd64)

### 验收标准
- [ ] `scripts/release.sh` 可以创建完整发布
- [ ] GitHub Release 包含多平台二进制
- [ ] Release 包含 SHA256 checksum
- [ ] `CHANGELOG.md` 自动更新

---

## 任务 G: 沙箱执行环境

### 目标
为 Lumen 的 bash 执行添加可选的沙箱隔离，防止恶意命令影响宿主机。

### 参考实现
**Claw Code** 的沙箱：
- GitHub: https://github.com/ultraworkers/claw-code
- 文件: `rust/crates/runtime/src/sandbox.rs` (384 LOC)
- 技术: Linux user namespace + network namespace

**旧 Lumen (Go)** 的沙箱：
- GitHub: https://github.com/exergyleizhou-ux/lumen
- 文件: `internal/sandbox/sandbox.go`
- 技术: Docker 容器 (`--read-only`, `--network=none`, 内存/CPU 限制)

**DeepSeek-Reasonix** 的沙箱：
- GitHub: https://github.com/esengine/DeepSeek-Reasonix
- macOS: `sandbox-exec` (Seatbelt)
- Linux: `bubblewrap`
- Windows: AppContainer

### 当前 Lumen 位置
- 已有 `lumen-guard` L0-L3（进程级命令拦截）
- 但无沙箱隔离（命令仍在用户权限下执行）

### 具体实现

#### G1. macOS 沙箱
新建文件: `agent/crates/codegen/lumen-guard/src/sandbox.rs`
使用 macOS `sandbox-exec`：
- 创建 Seatbelt profile（禁止网络、限制文件系统写路径）
- 在 bash 执行前应用 sandbox profile
- 参考 Reasonix 的 Seatbelt 实现

#### G2. Linux 沙箱
使用 `bubblewrap` 或 user namespace：
- 创建新的 mount namespace
- 绑定只读的 rootfs
- 限制网络访问

#### G3. 配置
```toml
[guard.sandbox]
enabled = false
mode = "strict"  # off | basic | strict
network = false  # 禁止网络
read_write_paths = ["<workspace>"]  # 允许写入的路径
```

### 验收标准
- [ ] macOS 上启用沙箱后，bash 命令无法访问网络
- [ ] 沙箱内无法写入白名单外的路径
- [ ] 沙箱性能开销 < 100ms
- [ ] 可配置关闭（默认关闭，兼容现有行为）

---

## 优先级建议

| 任务 | 影响 | 工作量 | 建议优先级 |
|------|------|--------|-----------|
| **A: 自动更新** | 高（用户每天用） | 小（1-2 天） | **P0** |
| **F: 发布流水线** | 高（团队协作） | 中（2-3 天） | **P0** |
| **C: 多模型协作** | 中（复杂任务） | 中（3-5 天） | **P1** |
| **B: MCP 插件** | 中（生态扩展） | 大（5-10 天） | **P1** |
| **G: 沙箱** | 中（安全增强） | 中（3-5 天） | **P2** |
| **D: 桌面应用** | 低（非核心） | 大（10-20 天） | **P3** |
| **E: IM 机器人** | 低（非核心） | 大（10-15 天） | **P3** |

## 参考链接汇总

| 项目 | GitHub | 关键路径 |
|------|--------|---------|
| 上游 Grok Build | https://github.com/xai-org/grok-build | 分叉点: `c68e39f` |
| 旧 Lumen (Go) | https://github.com/exergyleizhou-ux/lumen | `internal/sandbox/`, `internal/editverify/`, `desktop/` |
| Claw Code | https://github.com/ultraworkers/claw-code | `rust/crates/plugins/`, `rust/crates/runtime/src/sandbox.rs`, `rust/crates/runtime/src/plugin_lifecycle.rs` |
| DeepSeek-Reasonix | https://github.com/esengine/DeepSeek-Reasonix | `internal/agent/coordinator.go`, `desktop/updater.go`, `internal/bot/`, `desktop/main.go` |
