//! Multi-provider prompt-cache strategy (DeepSeek-first, portable).
//!
//! Reasonix optimized for DeepSeek automatic prefix cache. Lumen keeps that
//! as the **preferred** profile and adds explicit adaptation rules for other
//! backends so the same discipline does not silently claim "high hit" where
//! the provider cannot deliver it.

/// How a provider reuses prompt prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMechanism {
    /// Automatic server-side prefix cache (DeepSeek, many OpenAI models).
    AutomaticPrefix,
    /// Explicit breakpoints / cache_control markers (Anthropic Messages).
    ExplicitBreakpoints,
    /// Provider claims cache but client must not assert definitive hit without report.
    ReportedOnly,
    /// No cloud prompt cache (local Ollama / LM Studio / many small hosts).
    None,
}

/// Relative economic / reliability value of investing in prefix stability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheValue {
    /// Default product path — high savings when prefix is stable.
    High,
    /// Worth keeping prefix stable; savings vary by model tier.
    Medium,
    /// Discipline still good for determinism; little bill impact.
    Low,
    /// No provider cache; stability is hygiene only.
    None,
}

/// Per-binding cache profile used by status, defaults, and docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheProfile {
    pub mechanism: CacheMechanism,
    pub value: CacheValue,
    /// Short operator-facing label (e.g. "DeepSeek auto-prefix").
    pub label: &'static str,
    /// Concrete adaptation note for this family.
    pub adaptation: &'static str,
}

/// Infer a profile from model id / catalog key / base URL hints.
///
/// Intentionally heuristic and pure — no I/O. Host may override via config later.
pub fn profile_for_model(model_id: &str, base_url: Option<&str>) -> CacheProfile {
    let id = model_id.to_ascii_lowercase();
    let url = base_url.unwrap_or("").to_ascii_lowercase();

    if id.contains("deepseek") || url.contains("deepseek") {
        return CacheProfile {
            mechanism: CacheMechanism::AutomaticPrefix,
            value: CacheValue::High,
            label: "DeepSeek auto-prefix",
            adaptation: "Keep system+tools prefix byte-stable; put dynamic state on turn tail only. Highest Lumen ROI.",
        };
    }

    if id.contains("claude")
        || id.contains("anthropic")
        || url.contains("anthropic")
        || url.contains("minimax") && id.contains("minimax")
    {
        return CacheProfile {
            mechanism: CacheMechanism::ExplicitBreakpoints,
            value: CacheValue::Medium,
            label: "Anthropic-style breakpoints",
            adaptation: "Prefix stability helps, but mark long stable blocks with cache_control/ephemeral breakpoints when the Messages API is used; not automatic like DeepSeek.",
        };
    }

    if id.contains("gpt")
        || id.contains("o3")
        || id.contains("o4")
        || id.starts_with("openai")
        || url.contains("openai.com")
        || url.contains("api.x.ai")
        || id.contains("grok")
    {
        return CacheProfile {
            mechanism: CacheMechanism::AutomaticPrefix,
            value: CacheValue::Medium,
            label: "OpenAI-compatible auto-prefix",
            adaptation: "Many models auto-cache long stable prefixes; still keep tools/system stable. Hit rates are model-dependent — only show definitive % from provider-reported tokens.",
        };
    }

    if url.contains("127.0.0.1")
        || url.contains("localhost")
        || id.contains("ollama")
        || id.contains("lmstudio")
        || id.contains("vllm")
        || id.contains("local")
        || id.contains("exo")
    {
        return CacheProfile {
            mechanism: CacheMechanism::None,
            value: CacheValue::None,
            label: "local / no cloud cache",
            adaptation: "No cloud prompt-cache billing. Prefix stability still reduces local recompute where supported; never display provider cache % without reports.",
        };
    }

    // Moonshot / Qwen / GLM / generic OpenAI-compatible BYOK
    if id.contains("kimi")
        || id.contains("moonshot")
        || id.contains("qwen")
        || id.contains("glm")
        || id.contains("mimo")
        || url.contains("dashscope")
        || url.contains("bigmodel")
        || url.contains("moonshot")
    {
        return CacheProfile {
            mechanism: CacheMechanism::ReportedOnly,
            value: CacheValue::Medium,
            label: "OpenAI-compatible (report-gated)",
            adaptation: "Treat like auto-prefix when usage reports cache_read; otherwise estimate only. Do not claim high hit without provider numbers.",
        };
    }

    CacheProfile {
        mechanism: CacheMechanism::ReportedOnly,
        value: CacheValue::Low,
        label: "generic",
        adaptation: "Apply prefix discipline; only definitive cache display when provider returns cached tokens.",
    }
}

/// Whether the truth bar may claim a definitive cache hit for this profile
/// given that we *have* provider-reported tokens.
pub fn allows_definitive_display(profile: &CacheProfile, has_provider_tokens: bool) -> bool {
    if !has_provider_tokens {
        return false;
    }
    !matches!(profile.mechanism, CacheMechanism::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_is_high_auto() {
        let p = profile_for_model("deepseek-chat", Some("https://api.deepseek.com/v1"));
        assert_eq!(p.mechanism, CacheMechanism::AutomaticPrefix);
        assert_eq!(p.value, CacheValue::High);
    }

    #[test]
    fn claude_is_breakpoints() {
        let p = profile_for_model("claude-sonnet", None);
        assert_eq!(p.mechanism, CacheMechanism::ExplicitBreakpoints);
    }

    #[test]
    fn local_is_none() {
        let p = profile_for_model("ollama", Some("http://127.0.0.1:11434/v1"));
        assert_eq!(p.mechanism, CacheMechanism::None);
        assert_eq!(p.value, CacheValue::None);
    }

    #[test]
    fn openai_medium_auto() {
        let p = profile_for_model("gpt-4o", Some("https://api.openai.com/v1"));
        assert_eq!(p.mechanism, CacheMechanism::AutomaticPrefix);
        assert_eq!(p.value, CacheValue::Medium);
    }
}
