#!/usr/bin/env bash
# Structural proof: formal E0 model identities + legacy-compatible provider catalog.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

fail() { echo "FAIL: $*" >&2; exit 1; }

MODELS="$ROOT/agent/crates/codegen/xai-grok-models/default_models.json"
BIN_TOML="$ROOT/agent/crates/codegen/xai-grok-pager-bin/Cargo.toml"
EXAMPLE="$ROOT/config/lumen.example.toml"
DOC="$ROOT/docs/user/multi-provider.md"

test -f "$MODELS" || fail "missing $MODELS"
grep -q '"default": "deepseek-v4-pro"' "$MODELS" || fail "default_models.json default is not deepseek-v4-pro"
grep -q '"model": "deepseek-v4-pro"' "$MODELS" || fail "default_models.json missing deepseek-v4-pro"
# BYOK must be embedded so isolated GROK_HOME works without xAI login.
grep -q '"base_url": "https://api.deepseek.com/v1"' "$MODELS" || fail "default_models.json missing DeepSeek base_url"
grep -q '"env_key": "DEEPSEEK_API_KEY"' "$MODELS" || fail "default_models.json missing env_key DEEPSEEK_API_KEY"
grep -q '"byok": true' "$MODELS" || fail "default_models.json missing byok true"

# Full legacy Go catalog plus MiniMax from the Science provider table.
for id in \
  deepseek-v4-pro deepseek-v4-flash deepseek-chat deepseek-reasoner \
  openai-gpt4o openai-gpt4o-mini openai-gpt41 openai-o3-mini openai-o4-mini \
  claude-sonnet claude-opus claude-3.5-sonnet claude-3.5-haiku \
  grok-4.5 xai-grok grok-3-mini kimi-k2 moonshot-v1 \
  qwen-max qwen-plus qwen-turbo qwen-coder \
  glm-4 glm-4-flash glm-4-plus mimo-chat minimax-m3 \
  lmstudio ollama vllm exo local-openai; do
  grep -q "\"id\": \"$id\"" "$MODELS" || fail "default_models.json missing provider id=$id"
done
grep -q '"api_backend": "messages"' "$MODELS" || fail "default_models.json missing Anthropic messages backend"
grep -q 'api.mimo.run' "$MODELS" || fail "default_models.json missing legacy-true MiMo URL api.mimo.run"
if grep -q 'api.xiaomimimo.com' "$MODELS"; then
  fail "default_models.json still contains stale MiMo URL api.xiaomimimo.com"
fi
grep -q '"model": "qwen3:4b"' "$MODELS" || fail "Ollama default model is not qwen3:4b"
for port in 1234 11434 8000 52415; do
  grep -q "$port" "$MODELS" || fail "default_models.json missing local endpoint port $port"
done

python3 - "$MODELS" <<'PY' || fail "default_models.json structural audit failed"
import json
import sys

with open(sys.argv[1], encoding="utf-8") as f:
    root = json.load(f)

assert root["default"] == "deepseek-v4-pro"
assert root["web_search"] == "deepseek-v4-pro"
assert root["image_description"] == "deepseek-chat"
assert root["session_summary"] == "deepseek-v4-flash"
models = root["models"]
assert len(models) >= 29, len(models)
by_id = {m["id"]: m for m in models}
assert len(by_id) == len(models), "duplicate or missing ids"

deepseek = by_id["deepseek-v4-pro"]
assert deepseek["model"] == "deepseek-v4-pro"
assert deepseek["base_url"] == "https://api.deepseek.com/v1"
assert deepseek["env_key"] == "DEEPSEEK_API_KEY"
assert deepseek["byok"] is True
assert [e["id"] for e in deepseek["reasoning_efforts"]] == ["high", "max"]
assert by_id["deepseek-v4-flash"]["model"] == "deepseek-v4-flash"
assert by_id["grok-4.5"]["model"] == "grok-4.5"
assert not any("pricing" in model for model in models), "catalog pricing is not runtime-backed"
for alias in ["deepseek-chat", "deepseek-reasoner"]:
    assert by_id[alias]["hidden"] is True
    assert "2026-07-24 15:59 UTC" in by_id[alias]["description"]
    assert "V4 Flash" in by_id[alias]["description"]
assert "Pro" not in by_id["deepseek-reasoner"]["name"]

for model_id in [
    "claude-sonnet", "claude-opus", "claude-3.5-sonnet",
    "claude-3.5-haiku", "minimax-m3",
]:
    assert by_id[model_id]["api_backend"] == "messages", model_id

assert by_id["minimax-m3"]["base_url"] == "https://api.minimaxi.com/anthropic/v1"
assert by_id["mimo-chat"]["base_url"] == "https://api.mimo.run/v1"
assert by_id["ollama"]["model"] == "qwen3:4b"
assert not any(m["id"].startswith("gemini") for m in models)
for model_id, port in {
    "lmstudio": "1234", "ollama": "11434", "vllm": "8000", "exo": "52415"
}.items():
    assert port in by_id[model_id]["base_url"], model_id
    assert by_id[model_id]["api_backend"] == "chat_completions", model_id
PY

test -f "$BIN_TOML" || fail "missing pager-bin Cargo.toml"
grep -q 'name = "lumen"' "$BIN_TOML" || fail "binary name is not lumen"
grep -q 'default-run = "lumen"' "$BIN_TOML" || fail "default-run is not lumen"

test -f "$EXAMPLE" || fail "missing config/lumen.example.toml"
grep -q 'base_url = "https://api.deepseek.com/v1"' "$EXAMPLE" || fail "example missing DeepSeek base_url"
grep -q 'default = "deepseek-v4-pro"' "$EXAMPLE" || fail "example missing default deepseek-v4-pro"
grep -q 'auto_update = false' "$EXAMPLE" || fail "example missing auto_update = false"
for id in openai-gpt4o claude-sonnet kimi-k2 moonshot-v1 qwen-plus glm-4 mimo-chat minimax-m3 lmstudio ollama vllm exo; do
  grep -Fq "[model.$id]" "$EXAMPLE" || fail "example missing [model.$id]"
done
grep -Fq 'model = "qwen3:4b"' "$EXAMPLE" || fail "example Ollama model is not qwen3:4b"
for port in 1234 11434 8000 52415; do
  grep -q "$port" "$EXAMPLE" || fail "example missing local endpoint port $port"
done

python3 - "$EXAMPLE" <<'PY' || fail "config/lumen.example.toml parse/catalog audit failed"
import sys
try:
    import tomllib
except ModuleNotFoundError:  # macOS system Python < 3.11
    import tomli as tomllib

with open(sys.argv[1], "rb") as f:
    cfg = tomllib.load(f)
assert cfg["models"]["default"] == "deepseek-v4-pro"
models = cfg["model"]
for model_id in [
    "deepseek-v4-pro", "deepseek-v4-flash", "grok-4.5", "deepseek-chat",
    "openai-gpt41", "claude-3.5-sonnet", "grok-3-mini",
    "kimi-k2", "qwen-coder", "glm-4-flash", "mimo-chat", "minimax-m3",
    "lmstudio", "ollama", "vllm", "exo",
]:
    assert model_id in models, model_id
PY

test -f "$DOC" || fail "missing docs/user/multi-provider.md"
grep -q '能聊天不等于能驱动 agent' "$DOC" || fail "docs missing chat-vs-tool_call warning"
grep -q 'Gemini.*后置' "$DOC" || fail "docs must state native Gemini is deferred"
for key in DEEPSEEK_API_KEY OPENAI_API_KEY ANTHROPIC_API_KEY MOONSHOT_API_KEY DASHSCOPE_API_KEY ZHIPU_API_KEY MIMO_API_KEY MINIMAX_API_KEY; do
  grep -q "$key" "$DOC" || fail "docs missing environment variable $key"
done
test -x "$ROOT/scripts/probe-local.sh" || fail "missing executable scripts/probe-local.sh"
test -x "$ROOT/scripts/test-probe-local.sh" || fail "missing executable scripts/test-probe-local.sh"
test -f "$ROOT/docs/user/local-models.md" || fail "missing docs/user/local-models.md"

# Registry + update crate must agree: auto_update default false.
REG="$ROOT/agent/crates/codegen/xai-grok-pager/src/settings/registry.rs"
DEFS="$ROOT/agent/crates/codegen/xai-grok-pager/src/settings/defs.rs"
UPD="$ROOT/agent/crates/codegen/xai-grok-update/src/auto_update.rs"
grep -q 'auto_update.unwrap_or(false)' "$REG" || fail "registry auto_update must unwrap_or(false)"
grep -q 'SettingKind::Bool { default: false }' "$DEFS" || fail "defs auto_update default must be false"
grep -q 'configured.unwrap_or(false)' "$UPD" || fail "effective_auto_update must unwrap_or(false)"

echo "OK: E0 model identities and legacy-compatible catalog structural checks pass"
