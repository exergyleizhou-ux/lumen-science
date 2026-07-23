# 多模型适配（Multi-provider）

Lumen 的内置目录是多厂商 BYOK，不是 DeepSeek 单厂商产品。默认执行模型为正式
API ID `deepseek-v4-pro`；`deepseek-v4-flash` 是快速备选，`grok-4.5` 是可选的
高能力/多模态模型。长会话保持稳定前缀时，DeepSeek 的 **automatic prefix cache**
仍可降低成本。用户可以随时用 `-m`、`/model` 或 Ctrl+M 切换。

### Prompt cache 是否「全模型同等」？

**否。** 前缀稳定纪律对所有模型有益；**账单级高命中**主要在 DeepSeek（以及部分
OpenAI 兼容自动缓存模型）。Claude 等需要显式 breakpoint；本地模型通常没有云端
prompt cache。完整矩阵与纪律见 `policy/LUMEN_CACHE.md`（Reasonix 级 shape 诊断 +
多厂商适配，实现于 `lumen-discipline`）。

真实 API key 只放环境变量，不写进配置文件或 git。

## 协议边界

| `api_backend` | 协议 | 内置厂商 |
|---|---|---|
| `chat_completions` | OpenAI 兼容的 `/v1/chat/completions` | DeepSeek、OpenAI、xAI、Kimi/Moonshot、Qwen、GLM、MiMo、本地端点 |
| `messages` | Anthropic Messages | Claude、MiniMax Anthropic 兼容口 |
| `responses` | OpenAI Responses | xAI Grok 4.5 等支持该协议的模型 |

旧 Go Lumen 还有原生 Gemini wire kind；当前 Rust 目录没有等价的原生 Gemini
backend，因此 Gemini 明确后置，未写入 `default_models.json`。这不是原生 Gemini
已对齐，也不应把实验性的 OpenAI 兼容入口描述成完整等价实现。

## 内置目录

`agent/crates/codegen/xai-grok-models/default_models.json` 是编译进二进制的目录真源。
`config/lumen.example.toml` 给出相同家族的可复制配置。

| ID | API model / 家族 | Backend | 环境变量 |
|---|---|---|---|
| `deepseek-v4-pro`, `deepseek-v4-flash` | DeepSeek V4 正式 ID | `chat_completions` | `DEEPSEEK_API_KEY` |
| `deepseek-chat`, `deepseek-reasoner` | 仅兼容迁移：均路由 V4 Flash；后者是 thinking、不是 Pro；2026-07-24 15:59 UTC 退役 | `chat_completions` | `DEEPSEEK_API_KEY` |
| `openai-gpt4o`, `openai-gpt4o-mini`, `openai-gpt41` | GPT-4o / GPT-4.1 | `chat_completions` | `OPENAI_API_KEY` |
| `openai-o3-mini`, `openai-o4-mini` | o3-mini / o4-mini | `chat_completions` | `OPENAI_API_KEY` |
| `claude-sonnet`, `claude-opus` | Claude 4 | `messages` | `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN` |
| `claude-3.5-sonnet`, `claude-3.5-haiku` | Claude 3.5 | `messages` | `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN` |
| `grok-4.5` | Grok 4.5 | `responses` | `XAI_API_KEY`, `GROK_CODE_XAI_API_KEY`, `GROK_API_KEY` |
| `xai-grok`, `grok-3-mini` | Grok 3 legacy presets | `chat_completions` | `XAI_API_KEY`, `GROK_CODE_XAI_API_KEY`, `GROK_API_KEY` |
| `kimi-k3` | Kimi K3 via Kimi Code (`https://api.kimi.com/coding/v1`, API model `k3`) | `chat_completions` | `KIMI_CODE_API_KEY` |
| `kimi-k2`, `moonshot-v1` | Moonshot 开放平台（与 Kimi Code key/端点不互通） | `chat_completions` | `MOONSHOT_API_KEY` |
| `qwen-max`, `qwen-plus`, `qwen-turbo`, `qwen-coder` | Qwen / DashScope | `chat_completions` | `DASHSCOPE_API_KEY` |
| `glm-4`, `glm-4-flash`, `glm-4-plus` | Zhipu GLM | `chat_completions` | `ZHIPU_API_KEY`, `ZHIPUAI_API_KEY`, `BIGMODEL_API_KEY`, `GLM_API_KEY` |
| `mimo-chat` | MiMo (`https://api.mimo.run/v1`) | `chat_completions` | `MIMO_API_KEY` |
| `minimax-m3` | MiniMax-M3 | `messages` | `MINIMAX_API_KEY` |
| `lmstudio`, `ollama`, `vllm`, `exo`, `local-openai` | 本地 OpenAI 兼容 | `chat_completions` | 可选的本地占位 key，见下表 |

## 选择模型

```bash
# 当前调用
lumen -m openai-gpt41 "fix the flaky test"
lumen -m kimi-k3
lumen -m minimax-m3
lumen -m ollama

# TUI 内
/model claude-sonnet
/model lmstudio
```

持久默认可以在配置里覆盖；内置出厂默认仍是 DeepSeek：

```toml
[models]
default = "claude-sonnet"
```

旧 DeepSeek alias 只为已有配置迁移而保留，并在模型选择界面隐藏。请把
`deepseek-chat` 迁移为 `deepseek-v4-flash`；把 `deepseek-reasoner` 按所需质量迁移为
`deepseek-v4-flash` 或 `deepseek-v4-pro`，不能把 reasoner alias 解释成 Pro。

内置目录不保存会漂移且运行时并未消费的 `pricing` 字段，避免 UI 或审计把陈旧数字
误当成真实计费配置。按 DeepSeek 官方 2026-07-19 可见价格，每百万 token 的美元价为：
Flash 缓存命中输入 `$0.0028`、未命中输入 `$0.14`、输出 `$0.28`；Pro 分别为
`$0.003625`、`$0.435`、`$0.87`。价格可能调整，发布或成本核算时应重新核对
[官方模型与价格](https://api-docs.deepseek.com/quick_start/pricing)，而不是读取嵌入目录。

自定义厂商或租户端点：

```toml
[model.my-provider]
model = "their-model-id"
name = "My Provider"
base_url = "https://api.example.com/v1"
api_backend = "chat_completions"
env_key = "MY_PROVIDER_API_KEY"
```

## 本地模型

所有内置本地端点都走 OpenAI 兼容协议。端口与旧 Go Lumen 预设一致：

| ID | 默认 base URL | 默认 model | 环境变量 |
|---|---|---|---|
| `lmstudio` | `http://127.0.0.1:1234/v1` | `local-model` | `LMSTUDIO_API_KEY` 或 `LOCAL_API_KEY` |
| `ollama` | `http://127.0.0.1:11434/v1` | `qwen3:4b` | `OLLAMA_API_KEY` |
| `vllm` | `http://127.0.0.1:8000/v1` | `local-model` | `LOCAL_API_KEY` |
| `exo` | `http://127.0.0.1:52415/v1` | `local-model` | `LOCAL_API_KEY` |
| `local-openai` | `http://127.0.0.1:8000/v1` | `local-model` | `LOCAL_API_KEY` |

例如 Ollama：

```bash
ollama pull qwen3:4b
ollama serve
export OLLAMA_API_KEY=ollama  # 仅在客户端/端点要求非空值时需要
lumen -m ollama
```

### 能聊天不等于能驱动 agent

Lumen 的 agent 循环依赖模型真正发出 OpenAI 风格的 `tool_call`，例如
`read_file`、`edit_file` 和 `run_bash`。只返回自然语言的模型即使能聊天，也不能
完成文件修改或命令执行。对本地模型应把真实 tool-call 探测作为可用门槛，不能把
端口可达或普通聊天成功冒充 agent-ready。

如果本地服务暴露的模型名不是 `local-model`，请在用户配置中覆盖 `model`。不要在
文档或目录中预先声称未经实测的本地模型具备 tool-call 能力。

用仓库探测器做真实能力判定；完整退出码和安全边界见
[`local-models.md`](local-models.md)：

```bash
./scripts/probe-local.sh --list
./scripts/probe-local.sh --preset ollama --model qwen3:4b
./scripts/probe-local.sh --base-url http://127.0.0.1:1234/v1 --model local-model --json
```

## 安全与验证

- Key 放环境变量；提交前检查没有 `sk-...` 一类真实密钥。
- 切换厂商后至少做一个需要工具调用的最小任务，不能只测普通聊天。
- 目录结构门禁：`./scripts/assert-defaults.sh`。
- 本地 tool-call 探测器合同：`./scripts/test-probe-local.sh`。
- 编译时目录单测：在 `agent/` 下运行 `cargo test -p xai-grok-models --lib`。
