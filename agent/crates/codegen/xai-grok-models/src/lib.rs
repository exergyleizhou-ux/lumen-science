//! Default model IDs loaded from `default_models.json` at runtime.
//! Edit that JSON file to change them.
//!
//! At runtime each model is resolved via:
//!   CLI flag > ENV var > config.toml > remote settings > these defaults

use std::sync::LazyLock;

/// The raw JSON, embedded at compile time. Re-exported through the
/// `xai_grok_shell::models` facade and consumed by `agent::config`, so it must
/// be `pub` (was `pub(crate)` when this lived inside the shell crate).
pub const DEFAULT_MODELS_JSON: &str = include_str!("../default_models.json");

#[derive(serde::Deserialize)]
struct DefaultModels {
    default: String,
    /// Falls back to `default` if not specified in JSON.
    web_search: Option<String>,
    /// Falls back to `default` if not specified in JSON.
    image_description: Option<String>,
    /// Falls back to `default` if not specified in JSON.
    session_summary: Option<String>,
    models: Vec<DefaultModelEntry>,
}

#[derive(serde::Deserialize)]
struct DefaultModelEntry {
    model: String,
}

static DEFAULTS: LazyLock<DefaultModels> = LazyLock::new(|| {
    let defaults: DefaultModels = serde_json::from_str(DEFAULT_MODELS_JSON)
        .expect("default_models.json: invalid JSON or missing 'default' field");

    // Baked-in JSON — a mismatch here is a developer error, not a runtime condition.
    let model_ids: Vec<&str> = defaults.models.iter().map(|m| m.model.as_str()).collect();
    assert!(
        model_ids.contains(&defaults.default.as_str()),
        "default_models.json: 'default' is '{}' but 'models' array only has {model_ids:?}",
        defaults.default,
    );

    defaults
});

/// Primary model for coding tasks and general fallback.
pub fn default_model() -> &'static str {
    &DEFAULTS.default
}

/// Model for web search tool synthesis. Falls back to default model.
pub fn default_web_search_model() -> &'static str {
    DEFAULTS.web_search.as_deref().unwrap_or(&DEFAULTS.default)
}

/// Model for image describe. Falls back to default model.
pub fn default_image_description_model() -> &'static str {
    DEFAULTS
        .image_description
        .as_deref()
        .unwrap_or(&DEFAULTS.default)
}

/// Model for session title generation. Falls back to default model.
pub fn default_session_summary_model() -> &'static str {
    DEFAULTS
        .session_summary
        .as_deref()
        .unwrap_or(&DEFAULTS.default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn embedded_catalog() -> Value {
        serde_json::from_str(DEFAULT_MODELS_JSON).expect("embedded default_models.json parses")
    }

    fn model_by_id<'a>(models: &'a [Value], id: &str) -> &'a Value {
        models
            .iter()
            .find(|m| m.get("id").and_then(Value::as_str) == Some(id))
            .unwrap_or_else(|| panic!("embedded catalog missing model id={id}"))
    }

    #[test]
    fn lumen_default_model_is_deepseek_v4_pro() {
        assert_eq!(
            default_model(),
            "deepseek-v4-pro",
            "embedded default_models.json must default to the formal DeepSeek V4 Pro API ID"
        );
    }

    #[test]
    fn lumen_default_models_json_uses_formal_role_models() {
        let v = embedded_catalog();
        assert_eq!(v["default"], "deepseek-v4-pro");
        assert_eq!(v["web_search"], "deepseek-v4-pro");
        assert_eq!(v["image_description"], "deepseek-chat");
        assert_eq!(v["session_summary"], "deepseek-v4-flash");
        let models = v["models"].as_array().expect("models array");
        for id in ["deepseek-v4-pro", "deepseek-v4-flash"] {
            let deepseek = model_by_id(models, id);
            assert_eq!(deepseek["model"], id);
            assert_eq!(deepseek["base_url"], "https://api.deepseek.com/v1");
            assert_eq!(deepseek["env_key"], "DEEPSEEK_API_KEY");
            assert_eq!(deepseek["byok"], true);
            assert_eq!(deepseek["context_window"], 1_000_000);
            assert_eq!(deepseek["max_completion_tokens"], 384 * 1024);
            assert_eq!(deepseek["supports_reasoning_effort"], true);
            assert_eq!(deepseek["reasoning_effort"], "high");
            let efforts = deepseek["reasoning_efforts"].as_array().unwrap();
            assert_eq!(efforts.len(), 2);
            assert_eq!(efforts[0]["id"], "high");
            assert_eq!(efforts[1]["id"], "max");
        }

        let grok = model_by_id(models, "grok-4.5");
        assert_eq!(grok["model"], "grok-4.5");
        assert_eq!(grok["base_url"], "https://api.x.ai/v1");
        assert_eq!(grok["api_backend"], "responses");
        assert!(models.iter().all(|model| model.get("pricing").is_none()));
    }

    #[test]
    fn legacy_deepseek_aliases_are_hidden_and_truthfully_deprecated() {
        let v = embedded_catalog();
        let models = v["models"].as_array().expect("models array");
        let chat = model_by_id(models, "deepseek-chat");
        let reasoner = model_by_id(models, "deepseek-reasoner");

        for legacy in [chat, reasoner] {
            assert_eq!(legacy["hidden"], true);
            let description = legacy["description"].as_str().unwrap();
            assert!(description.contains("2026-07-24 15:59 UTC"));
            assert!(description.contains("V4 Flash"));
        }
        assert!(reasoner["name"].as_str().unwrap().contains("Flash"));
        assert!(!reasoner["name"].as_str().unwrap().contains("Pro"));
        assert!(
            reasoner["description"]
                .as_str()
                .unwrap()
                .contains("not V4 Pro")
        );
    }

    #[test]
    fn kimi_k3_uses_kimi_code_api_contract() {
        let v = embedded_catalog();
        let models = v["models"].as_array().expect("models array");
        let kimi = model_by_id(models, "kimi-k3");

        assert_eq!(kimi["model"], "k3");
        assert_eq!(kimi["base_url"], "https://api.kimi.com/coding/v1");
        assert_eq!(kimi["api_backend"], "chat_completions");
        assert_eq!(kimi["env_key"], "KIMI_CODE_API_KEY");
        assert_eq!(kimi["byok"], true);
        assert_eq!(kimi["supports_reasoning_effort"], true);
        assert_eq!(kimi["reasoning_effort"], "high");
        assert_eq!(kimi["context_window"], 262_144);
        let efforts = kimi["reasoning_efforts"].as_array().unwrap();
        assert_eq!(efforts.len(), 3);
        assert_eq!(efforts[0]["id"], "low");
        assert_eq!(efforts[1]["id"], "high");
        assert_eq!(efforts[2]["id"], "max");
    }

    #[test]
    fn lumen_catalog_covers_legacy_go_families_and_science_minimax() {
        let v = embedded_catalog();
        let models = v["models"].as_array().expect("models array");
        assert!(
            models.len() >= 29,
            "legacy-complete catalog should contain at least 29 entries; got {}",
            models.len()
        );
        let ids: Vec<&str> = models
            .iter()
            .map(|m| m["id"].as_str().expect("every catalog entry has an id"))
            .collect();
        for need in [
            "deepseek-v4-pro",
            "deepseek-v4-flash",
            "deepseek-chat",
            "deepseek-reasoner",
            "grok-4.5",
            "openai-gpt4o",
            "openai-gpt4o-mini",
            "openai-gpt41",
            "openai-o3-mini",
            "openai-o4-mini",
            "claude-sonnet",
            "claude-opus",
            "claude-3.5-sonnet",
            "claude-3.5-haiku",
            "xai-grok",
            "grok-3-mini",
            "kimi-k2",
            "kimi-k3",
            "moonshot-v1",
            "qwen-max",
            "qwen-plus",
            "qwen-turbo",
            "qwen-coder",
            "glm-4",
            "glm-4-flash",
            "glm-4-plus",
            "mimo-chat",
            "minimax-m3",
            "lmstudio",
            "ollama",
            "vllm",
            "exo",
            "local-openai",
        ] {
            assert!(
                ids.contains(&need),
                "catalog missing provider preset id={need}; have {ids:?}"
            );
        }
    }

    #[test]
    fn lumen_catalog_uses_correct_protocols_and_legacy_true_endpoints() {
        let v = embedded_catalog();
        let models = v["models"].as_array().expect("models array");

        for id in [
            "claude-sonnet",
            "claude-opus",
            "claude-3.5-sonnet",
            "claude-3.5-haiku",
            "minimax-m3",
        ] {
            assert_eq!(model_by_id(models, id)["api_backend"], "messages", "{id}");
        }
        assert_eq!(
            model_by_id(models, "minimax-m3")["base_url"],
            "https://api.minimaxi.com/anthropic/v1"
        );
        assert_eq!(
            model_by_id(models, "mimo-chat")["base_url"],
            "https://api.mimo.run/v1"
        );
        assert!(
            !DEFAULT_MODELS_JSON.contains("api.xiaomimimo.com"),
            "stale MiMo endpoint must not remain in embedded JSON"
        );
    }

    #[test]
    fn lumen_local_catalog_has_expected_ports_and_agent_candidate_model() {
        let v = embedded_catalog();
        let models = v["models"].as_array().expect("models array");

        for (id, port) in [
            ("lmstudio", "1234"),
            ("ollama", "11434"),
            ("vllm", "8000"),
            ("exo", "52415"),
        ] {
            let model = model_by_id(models, id);
            assert_eq!(model["api_backend"], "chat_completions", "{id}");
            assert!(
                model["base_url"]
                    .as_str()
                    .unwrap_or_default()
                    .contains(port),
                "{id} base_url must contain port {port}"
            );
        }
        assert_eq!(model_by_id(models, "ollama")["model"], "qwen3:4b");
    }
}
