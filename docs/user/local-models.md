# 本地模型：以 tool_call 为准

Lumen 可以连接 LM Studio、Ollama、vLLM、exo 和任意本地 OpenAI 兼容端点，
但端口可达或普通聊天成功都不代表模型能驱动 coding agent。可用门槛是模型在收到
工具 schema 后真正返回 OpenAI `tool_calls`。

## 快速探测

```bash
cd ~/code/lumen

# 查看内置本地预设，不发网络请求
./scripts/probe-local.sh --list

# 探测所有本地预设；未启动的端点会如实记为 unreachable
./scripts/probe-local.sh

# 只测 Ollama，或指定实际模型名
./scripts/probe-local.sh --preset ollama --model qwen3:4b

# 测一个自定义 localhost 端点并保存机器可读证据
./scripts/probe-local.sh \
  --base-url http://127.0.0.1:1234/v1 \
  --model qwen3:4b \
  --json > probe-local.json
```

探测发送一个最小编辑任务和唯一的 `edit_file` 工具。只有响应里存在结构化
`tool_calls` 才会给出 `can_tool_call=true`；回复“我会编辑文件”仍是失败。

## 退出码

| 退出码 | 含义 |
|---|---|
| `0` | 至少一个被测模型发出了真实 `tool_call` |
| `1` | 端点可达，但没有任何模型发出 `tool_call`（含 prose-only / HTTP 拒绝） |
| `2` | 所有端点都不可达，或探测前置条件无效 |

默认只允许 loopback URL，避免把本地占位 key 意外发到远端。确需测试远端兼容端点时，
必须显式传 `--allow-remote`；key 只能通过 `--api-key-env 环境变量名` 读取，输出不会包含值。

## Ollama 示例

```bash
ollama pull qwen3:4b
ollama serve
export OLLAMA_API_KEY=ollama  # 只有端点要求非空 Authorization 时才需要
./scripts/probe-local.sh --preset ollama --model qwen3:4b

# 通过后才进入真实 agent smoke
lumen -m ollama --single "读取 README.md，并用工具报告第一行" --always-approve
```

不要把探测脚本的 fixture 测试当成模型 live 证据。fixture 只验证解析器能区分
tool-call、prose-only 和 unreachable；真实模型结论必须来自上述 localhost 探测。

当前默认使用 Ollama 官方标注支持 tools 的 `qwen3:4b`。若其他模型（例如某些
`qwen2.5-coder` 量化版本）只把函数名和参数打印成普通 JSON 文本，探测会保持失败；
这种输出不能直接驱动 Agent，也不能手工改成绿色证据。
