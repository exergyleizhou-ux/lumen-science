package runtime

import (
	"fmt"
	"strings"

	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/proxy"
)

// ResolvedSpec bundles upstream spec + API key from active profile or legacy config.
type ResolvedSpec struct {
	Spec        proxy.ProviderSpec
	APIKey      string
	Adapter     string
	APIFormat   string
	BaseURL     string
	Model       string
	ProfileID   string
	ProfileName string
}

// ResolveActiveSpec maps active profile (or legacy provider slot) to a proxy spec.
func ResolveActiveSpec(cfg sciconfig.File) (ResolvedSpec, error) {
	if p := cfg.ActiveProfile(); p != nil {
		return resolveProfile(*p)
	}
	provider := strings.ToLower(strings.TrimSpace(cfg.Provider))
	if provider == "" {
		provider = sciconfig.DefaultProvider
	}
	spec, ok := proxy.LookupProvider(provider)
	if !ok {
		return ResolvedSpec{}, fmt.Errorf("unsupported provider %q", provider)
	}
	key := cfg.KeyFor(provider)
	if key == "" {
		return ResolvedSpec{}, fmt.Errorf("missing API key for %s", provider)
	}
	return ResolvedSpec{Spec: spec, APIKey: key, Adapter: provider, APIFormat: string(spec.Mode)}, nil
}

func resolveProfile(p sciconfig.Profile) (ResolvedSpec, error) {
	tpl, ok := sciconfig.TemplateByID(p.TemplateID)
	if !ok {
		tpl, _ = sciconfig.TemplateByID("custom")
	}
	key := strings.TrimSpace(p.APIKey)
	if key == "" {
		return ResolvedSpec{}, fmt.Errorf("profile %q has no API key", p.Name)
	}
	baseURL := sciconfig.ResolveProfileBaseURL(p)
	model := strings.TrimSpace(p.Model)
	adapter := tpl.Adapter

	base := ResolvedSpec{
		APIKey:      key,
		Adapter:     adapter,
		APIFormat:   tpl.APIFormat,
		BaseURL:     baseURL,
		Model:       model,
		ProfileID:   p.ID,
		ProfileName: p.Name,
	}

	switch adapter {
	case "relay":
		models := make([]proxy.ModelEntry, 0, len(tpl.BuiltinModels))
		for _, m := range tpl.BuiltinModels {
			models = append(models, proxy.ModelEntry{ID: m, DisplayName: m})
		}
		// Prefer explicit model; else first builtin when RequiresModelOverride.
		override := model
		if override == "" && tpl.RequiresModelOverride && len(tpl.BuiltinModels) > 0 {
			override = tpl.BuiltinModels[0]
		}
		spec := proxy.RelaySpec(baseURL, key, override, models)
		if tpl.ThinkingPolicy != "" {
			spec.ThinkingPolicy = tpl.ThinkingPolicy
		}
		base.Spec = spec
		base.Adapter = "relay"
		return base, nil

	case "openai-custom":
		if strings.TrimSpace(baseURL) == "" {
			return ResolvedSpec{}, fmt.Errorf("profile %q: custom OpenAI requires base_url", p.Name)
		}
		if proxy.LooksLikeAnthropicBase(baseURL) {
			return ResolvedSpec{}, fmt.Errorf("profile %q: base_url looks like Anthropic (/anthropic); use 自定义 Anthropic template instead", p.Name)
		}
		if model == "" {
			return ResolvedSpec{}, fmt.Errorf("profile %q: custom OpenAI requires model", p.Name)
		}
		base.Spec = proxy.OpenAIChatSpec(baseURL, model)
		return base, nil

	case "openai-responses":
		if strings.TrimSpace(baseURL) == "" {
			return ResolvedSpec{}, fmt.Errorf("profile %q: custom OpenAI Responses requires base_url", p.Name)
		}
		if proxy.LooksLikeAnthropicBase(baseURL) {
			return ResolvedSpec{}, fmt.Errorf("profile %q: base_url looks like Anthropic; use 自定义 Anthropic template", p.Name)
		}
		if model == "" {
			return ResolvedSpec{}, fmt.Errorf("profile %q: custom OpenAI Responses requires model", p.Name)
		}
		base.Spec = proxy.OpenAIResponsesSpec(baseURL, model)
		return base, nil

	default:
		spec, ok := proxy.LookupProvider(adapter)
		if !ok {
			return ResolvedSpec{}, fmt.Errorf("unknown adapter %q", adapter)
		}
		if tpl.BaseURLEditable && baseURL != "" && baseURL != tpl.BaseURL {
			spec.URL = strings.TrimRight(baseURL, "/") + "/v1/messages"
			if spec.Mode == proxy.ModeOpenAI {
				root := proxy.NormalizeOpenAIBase(baseURL)
				spec.URL = root + "/chat/completions"
			}
			if spec.Mode == proxy.ModeResponses {
				root := proxy.NormalizeOpenAIBase(baseURL)
				spec.URL = root + "/responses"
			}
		}
		if model != "" {
			spec.DefaultModel = model
		}
		if tpl.ThinkingPolicy != "" {
			spec.ThinkingPolicy = tpl.ThinkingPolicy
		}
		base.Spec = spec
		return base, nil
	}
}

// SpecFingerprint is a stable key for proxy reuse decisions (includes protocol semantics).
func SpecFingerprint(r ResolvedSpec) string {
	return strings.Join([]string{
		r.Adapter,
		r.APIFormat,
		r.BaseURL,
		r.Model,
		string(r.Spec.Mode),
		r.Spec.ForceModel,
		r.Spec.ThinkingPolicy,
		r.ProfileID,
	}, "|")
}
