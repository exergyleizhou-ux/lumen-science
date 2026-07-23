# Lumen

终端 coding agent：**Grok Build 体验身体** + **多模型 BYOK**（默认 DeepSeek 高缓存）+ Lumen 安全/纪律/自修。

- 方案：`docs/masterplan/`（权威：桌面 FINAL-2.0）
- 运行时：`agent/`（Grok pin，~135 万行 Rust）
- 二进制：`lumen`（UI/交互仍是 Grok TUI，产品名 Lumen）



## Repository history note

This `main` branch is the **Rust** Lumen product (Grok Build derivative).

The earlier **Go** Lumen line that previously occupied GitHub `main` is preserved at:

- branch: `archive/go-main` (tip was `dd8d71c`, includes Go v1.x releases)
- historical tags such as `v1.1.2` remain available for old release assets

Do not treat Go tags as the current shipping line. Current product version is in root `VERSION` (`0.1.220-alpha.4`).

## 快速开始

```bash
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"
export DEEPSEEK_API_KEY='你的key'   # 勿提交到 git

# 构建并安装到 ~/.local/bin
./scripts/install-local.sh
lumen --version
lumen --help

# 交互（在项目目录）
cd /path/to/your/project
lumen

# 单轮 headless
lumen --single "修 README 里的笔误" --always-approve
```

自用日记（15 日门禁）：复制 `journal/TEMPLATE-productivity-day.md` → `journal/YYYY-MM-DD.md`。

## 默认行为

| 项 | 值 |
|----|-----|
| 默认模型 | `deepseek-v4-pro`（正式 API ID；默认编码执行器，不是唯一后端） |
| 其它预设 | OpenAI / Claude / xAI / GLM / Qwen / MiMo / Ollama / 本地 OpenAI 兼容 |
| 遥测 Mixpanel | 默认关 |
| auto_update | 默认关 |
| 安全 | hard-deny（YOLO 也拦） |

```bash
lumen -m openai-gpt4o "..."
lumen -m claude-sonnet
lumen -m ollama          # 本地 ollama serve
# TUI: /model 或 Ctrl+M
```

配置示例：`config/lumen.example.toml` · 说明：`docs/user/multi-provider.md`。

本地模型必须先证明能发出真实工具调用：

```bash
./scripts/probe-local.sh --list
./scripts/probe-local.sh --preset ollama --model qwen3:4b
```

详见 `docs/user/local-models.md`；普通聊天成功不能当作 agent-ready。

Private beta 证据路径：Science 三步实跑见 `scripts/dogfood-science.sh`，首次用户
10 分钟路径见 `docs/user/10-minute-onboarding-evidence.md`。模板本身不算真人证据。

## 门禁脚本

```bash
cd ~/code/lumen
./scripts/assert-defaults.sh
./scripts/smoke-security.sh
./scripts/smoke-deepseek.sh          # L0；委托 E0 canonical current-checkout harness
./scripts/smoke-deepseek-agent.sh    # L1 tool
./scripts/verify-readiness.sh        # 汇总 readiness（需 key 跑 live 项）
```

| 脚本 | 作用 |
|------|------|
| `smoke-deepseek-l2/l3/l4/l5.sh` | Agent readiness 分层 |
| `eval-coding.sh` | 20 题 broken harness |
| `smoke-verify.sh` | 改后自修 CLI |
| `parity-run.sh` | CC 行为对照 |

## 体验说明

- **UI / 快捷键 / 审批 / session**：Grok Build TUI（未自建第二套界面）
- **品牌**：`--version` / `--help` 显示 **Lumen**；内部 crate 名仍可能带 `xai-grok-*`（后期 rename 可选）
- **ready**：`artifacts/readiness/status.json`；全自动门禁过后仍可能因 **15 日自用** 等人的门禁保持 BLOCKED

## 法律

Apache-2.0 衍生自 SpaceXAI Grok Build 开源树。见 `NOTICE`、`LEGAL.md`、`agent/UPSTREAM.md`。
