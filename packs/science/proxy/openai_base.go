package proxy

import "strings"

// NormalizeOpenAIBase cleans a user-supplied OpenAI-compatible base root.
// Accepts bare hosts, .../v1, .../chat/completions, .../responses, .../models.
func NormalizeOpenAIBase(base string) string {
	b := strings.TrimSpace(base)
	b = strings.TrimRight(b, "/")
	if b == "" {
		return b
	}
	lower := strings.ToLower(b)
	for _, suffix := range []string{
		"/chat/completions",
		"/completions",
		"/responses",
		"/models",
	} {
		if strings.HasSuffix(lower, suffix) {
			b = b[:len(b)-len(suffix)]
			b = strings.TrimRight(b, "/")
			lower = strings.ToLower(b)
			break
		}
	}
	// Bare host or root without a version segment → append /v1 (OpenAI convention).
	if !hasVersionSegment(b) {
		b = b + "/v1"
	}
	return b
}

func hasVersionSegment(base string) bool {
	// .../v1 or .../v4 etc. as final path segment
	i := strings.LastIndex(base, "/")
	if i < 0 || i == len(base)-1 {
		return false
	}
	seg := base[i+1:]
	if len(seg) >= 2 && (seg[0] == 'v' || seg[0] == 'V') {
		for _, c := range seg[1:] {
			if c < '0' || c > '9' {
				return false
			}
		}
		return true
	}
	// DashScope style: .../compatible-mode/v1 already has v1
	return strings.Contains(base, "/v1") || strings.Contains(base, "/v4")
}

// LooksLikeAnthropicBase reports whether a URL is clearly an Anthropic-compat path
// (user likely picked the wrong custom-openai template).
func LooksLikeAnthropicBase(base string) bool {
	lower := strings.ToLower(strings.TrimSpace(base))
	return strings.Contains(lower, "/anthropic")
}

// OpenAIChatSpec builds a ModeOpenAI provider for a custom chat-completions root.
func OpenAIChatSpec(baseURL, model string) ProviderSpec {
	base := NormalizeOpenAIBase(baseURL)
	display := model
	if display == "" {
		display = "custom-model"
	}
	return ProviderSpec{
		Name:         "openai-custom",
		Mode:         ModeOpenAI,
		URL:          base + "/chat/completions",
		KeyEnv:       "LUMEN_OPENAI_API_KEY",
		DefaultCap:   8192,
		DefaultModel: model,
		ForceModelOverride: model != "",
		ForceModel:         model,
		Models: []ModelEntry{
			{ID: "claude-opus-4-8", DisplayName: display},
		},
		ModelMap: map[string]string{
			"claude-opus-4-8":   model,
			"claude-sonnet-5":   model,
			"claude-sonnet-4-6": model,
			"claude-haiku-4-5":  model,
		},
	}
}

// OpenAIResponsesSpec builds a ModeResponses provider (Anthropic↔Responses translate).
func OpenAIResponsesSpec(baseURL, model string) ProviderSpec {
	base := NormalizeOpenAIBase(baseURL)
	display := model
	if display == "" {
		display = "custom-model"
	}
	return ProviderSpec{
		Name:               "openai-responses",
		Mode:               ModeResponses,
		URL:                base + "/responses",
		KeyEnv:             "LUMEN_OPENAI_API_KEY",
		DefaultCap:         8192,
		DefaultModel:       model,
		ForceModelOverride: model != "",
		ForceModel:         model,
		Models: []ModelEntry{
			{ID: "claude-opus-4-8", DisplayName: display},
		},
		ModelMap: map[string]string{
			"claude-opus-4-8":   model,
			"claude-sonnet-5":   model,
			"claude-sonnet-4-6": model,
			"claude-haiku-4-5":  model,
		},
	}
}
