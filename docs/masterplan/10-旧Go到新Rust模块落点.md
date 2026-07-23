# 10 — 旧 Go Lumen → 新 Rust（Grok Build）模块落点

> **旧仓（只读知识源）：** `~/lumen`（Go 为主，少量 Python）  
> **新仓（产品主路径）：** `~/code/lumen`（`agent/` = Grok Build Rust ≈ 百万行）  
> **原则：** 迁**语义 / 预设 / 门禁 / 垂直**，不整仓把 Go agent TUI 搬进 Rust；不重写 pager 主循环。

---

## 0. 总览（先看这张）

```
旧 Go Lumen                         新 Lumen (Grok Rust + packs)
─────────────────                   ────────────────────────────
internal/tui + agent 主循环    →    agent/crates/.../xai-grok-pager + shell  (已有身体)
internal/provider/*            →    xai-grok-sampler + default_models.json + config.toml
internal/config/model_presets  →    xai-grok-models/default_models.json     (配置迁)
internal/guard                 →    lumen-guard + permission/bash 路径      (语义迁)
internal/editverify            →    lumen-verify + hooks / PostTool         (语义迁)
internal/science               →    packs/science (+ 可选 MCP)              (包迁，非整 runtime)
internal/oasis / quant         →    packs/oasis · packs/quant
internal/eval · evals/         →    evals/ + scripts/eval-coding*.sh
skills / MCP cmds              →    skills/ · packs · Grok MCP
internal/modelpool             →    暂不整迁；靠多 preset + 用户 -m 切换
localprobe / ollama 矩阵       →    docs/user + 本地 preset + scripts/probe-local.sh
Python 科研/脚本               →    外围工具，不当 agent runtime
```

| 策略 | 含义 |
|------|------|
| **已有** | Grok 自带，基本不用从 Go 再写一遍 |
| **配置迁** | 只搬 URL / model id / env key / 文档 |
| **语义迁** | 规格与测试思想进 Rust 小 crate 或 hooks |
| **包迁** | 垂直能力以独立二进制/MCP 挂上 |
| **不迁 / 后置** | 明确砍掉或 v1.1+ |
| **仍 Go** | 旧仓继续只读参考；或垂直仍可独立 Go 维护再挂 MCP |

---

## 1. 核心运行时（身体）

| 旧 Go | 新落点 | 策略 | 说明 |
|-------|--------|------|------|
| `internal/tui`、lineedit、render | `xai-grok-pager*` | **已有** | 交互体验主战场；禁止为优雅重写主循环 |
| agent 主循环 / steps | `xai-grok-shell` / `xai-grok-agent` | **已有** | tool 循环、session、subagent 已在 Grok |
| `internal/stream` | sampler + pager 流式 | **已有** | |
| `internal/permission` | workspace permission + rules | **已有 + 语义加强** | deny 优先于 bypass |
| `internal/sandbox` | `xai-grok-sandbox` | **已有** | |
| `internal/tool` / toolkit | grok tools + MCP | **已有** | 旧 100+ 工具不必 1:1 |
| `cmd/lumen` 入口 | `xai-grok-pager-bin` → 二进制名 `lumen` | **已有** | 品牌层 |
| 配置 `lumen.toml` | `~/.grok/config.toml` + `config/lumen.example.toml` | **配置迁** | 键名不同，语义对齐 |

**白话：** 以前你们自己造壳；现在壳用 Grok，你们往壳上**贴纪律和默认**。

---

## 2. 模型 / Provider（你最关心的）

| 旧 Go | 新落点 | 策略 | 说明 |
|-------|--------|------|------|
| `internal/provider` 抽象 | `xai-grok-sampler` 多 backend | **已有** | `chat_completions` / `messages` / `responses` |
| `provider/openai` | 同上 OpenAI 兼容路径 | **已有** | DeepSeek / xAI / Qwen / GLM / Ollama 都走这 |
| `provider/anthro` | `api_backend = "messages"` | **已有** | Claude、部分 MiniMax Anthropic 口 |
| `provider/gemini` | 无原生 Gemini kind | **配置迁 / 实验** | 可用 Google OpenAI 兼容口；完整 Gemini 协议不整迁 |
| `config/model_presets.go` | `default_models.json` + example.toml | **配置迁（真源）** | 旧表最全：Kimi/lmstudio/vllm/exo/MiMo… |
| `lumen.toml` `[[providers]]` | `[model.*]` 段 | **配置迁** | env：`DEEPSEEK_API_KEY` 等 |
| Science `BuiltInProviders` | coding 目录 + 可选 packs/science 代理 | **拆开** | coding agent 用 OpenAI 口；Science 桥仍可独立 |
| `internal/modelpool` | — | **后置** | 故障转移/成本路由；v1 用手动 `-m` 即可 |
| `docs/local-models.md` | `docs/user/multi-provider.md` + local 专文 | **文档迁** | **能聊天 ≠ tool_call** 必须保留 |
| Ollama / LM Studio / vLLM / exo | 本地 preset base_url | **配置迁** | 端口：11434 / 1234 / 8000 / 52415 |
| Reasonix providers 示例 | 文档 + DeepSeek 默认 | **语义迁** | cache / effort；不嵌 Reasonix 整仓 |

**env 名对齐（旧→新继续沿用）：**

| Provider | 环境变量 |
|----------|----------|
| DeepSeek | `DEEPSEEK_API_KEY` |
| OpenAI | `OPENAI_API_KEY` |
| Anthropic | `ANTHROPIC_API_KEY` |
| xAI | `XAI_API_KEY` / `GROK_API_KEY` |
| 通义 | `DASHSCOPE_API_KEY` |
| 智谱 | `ZHIPU_API_KEY` / `ZHIPUAI_API_KEY` |
| Moonshot | `MOONSHOT_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| MiMo | `MIMO_API_KEY`（旧 base：`https://api.mimo.run/v1`） |

---

## 3. 安全 / 纪律 / 自修

| 旧 Go | 新落点 | 策略 |
|-------|--------|------|
| `internal/guard`（5+1、writepath、零宽） | `lumen-guard` + bash permission | **语义迁** |
| deny vs YOLO | permission rules | **已有 + 测** |
| Storm / RepeatSuccess | shell 循环或 hooks | **语义迁**（部分已有 lumen-discipline） |
| Delivery / 假完成 | `lumen-discipline` + goal gate | **语义迁** |
| `internal/editverify` | `lumen-verify` + PostTool | **语义迁** |
| Reasonix cache 纪律 | prompt 前缀纪律 + UI cache 行 | **语义迁**（DeepSeek 最贴） |
| `internal/policy` | `policy/*.md` + 代码 hard-deny | **文档+代码** |

---

## 4. 垂直与外围（不必进 pager）

| 旧 Go | 新落点 | 策略 |
|-------|--------|------|
| `internal/science` + science GUI | `packs/science` | **包迁** |
| Science proxy（CSSwitch 对标） | packs 内代理或独立进程 | **包迁 / 后置**；不塞进 pager |
| `cmd/lumen-mcp-*` | MCP 注册进 Grok | **包迁** |
| `internal/oasis` | `packs/oasis` | **包迁** |
| `internal/quant` | `packs/quant` | **包迁** |
| `internal/localprobe` | 可选 `scripts/probe-local.sh` | **后补** |
| `internal/eval` / modeleval | `evals/` + `eval-coding*.sh` | **配置+脚本迁** |
| `internal/websearch` | Grok 内置 web 工具 + env key | **已有** |
| `internal/memory` / lumenstore | `xai-grok-memory`（有限） | **部分已有**；不迁「99K 玄学」 |
| `internal/server` HTTP API | 一般不迁 | **后置**；主路径是 TUI |
| `desktop/lumen-lab` 等 | 独立产品线 | **不进 coding runtime** |
| Python 脚本 / LangGraph | 仓库外或 scripts | **外围** |

---

## 5. 明确「仍 Go / 只读 / 不迁」

| 项 | 原因 |
|----|------|
| 旧 `~/lumen` 整仓当 runtime | FINAL-2.0：Grok 唯一身体 |
| Go TUI 再维护一版 | 双 TUI 禁止 |
| 完整 Gemini 原生协议 | 新引擎无对等 kind；要则 OpenAI 兼容或后置 |
| modelpool 智能路由 | v1 非刚需 |
| Guardian 每工具 LLM 审查 | 成本/误杀；v1.1 可选 |
| 全量 `xai-*` crate 改名 | 单独里程碑 |
| CSSwitch Rust gateway 嵌入 | 旧计划已否；行为可对标不可嵌二进制 |

---

## 6. 语言怎么分工（落地纪律）

| 语言 | 干什么 |
|------|--------|
| **Rust** | 主 agent、TUI、session、工具执行、采样协议 |
| **Go** | 旧仓只读；垂直 packs 若仍是 Go 二进制可继续编译挂 MCP/PATH |
| **Python** | 评测、数据、科研流水线；不进主 runtime |
| **TOML/JSON/MD** | 模型预设、政策、门禁——**迁移成本最低、优先做** |

---

## 7. 推荐迁移顺序（若继续合仓）

1. **配置真源对齐：** 旧 `model_presets.go` + science `BuiltInProviders` → `default_models.json`（Kimi / MiniMax / lmstudio / vllm / exo / 正确 MiMo URL）  
2. **文档：** `local-models.md` 能力矩阵思想 → `docs/user/`  
3. **垂直 doctor：** packs 三步可达已有则加固  
4. **可选：** `probe-local` 等价 smoke（测 tool_call，不只聊天）  
5. **不碰：** pager 主循环、整迁 Science GUI  

---

## 8. 一句话

- **旧 Go：** 模型清单、国产 API、Science/本地推理、安全基因的**知识库**。  
- **新 Rust：** 已经能天天用的 **coding agent 身体**。  
- **正确合：** 预设与纪律进配置/小 crate；垂直旁路挂载；**不要**用 Go 重写一百万行 TUI，也**不要**指望 Go 仓自动变成 Rust。
