# M0 skeptic fix — 2026-07-16

## Rejected claim
Prior smoke claimed DeepSeek while using xAI session (grok-4.5 path).

## Fixes
1. `default_models.json`: base_url=api.deepseek.com/v1, env_key=DEEPSEEK_API_KEY, byok=true
2. `config.rs` `default_models()` honors embedded base_url/env_key/byok
3. `registry.rs` / `defs.rs`: auto_update unwrap_or(false) + UI default false
4. `smoke-deepseek.sh`: isolated GROK_HOME, prove model+host; 401 on DeepSeek = routing OK

## Proof
- Live call hit `https://api.deepseek.com/v1/chat/completions`
- Model: deepseek-chat, Auth: ApiKey
- No "Not signed in" / no grok-4.5
- Current DEEPSEEK_API_KEY returns 401 (invalid key) — ops to refresh; code path proven

## Tests
- `scripts/assert-defaults.sh` OK
- `cargo test -p xai-grok-models` OK
- `cargo test -p xai-grok-update --lib lumen_auto_update` OK
- release rebuild OK
