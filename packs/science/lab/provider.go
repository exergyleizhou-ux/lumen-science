package lab

import (
	"fmt"
	"os"
	"strings"

	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/proxy"
)

// ApplyScienceProfile sets provider API key env vars from the active science profile.
func ApplyScienceProfile(cfg sciconfig.File) (masked string, adapter string, err error) {
	p := cfg.ActiveProfile()
	key := ""
	templateID := cfg.Provider
	baseURL := ""
	model := ""
	if p != nil {
		key = strings.TrimSpace(p.APIKey)
		templateID = p.TemplateID
		if templateID == "" {
			templateID = cfg.Provider
		}
		baseURL = sciconfig.ResolveProfileBaseURL(*p)
		model = strings.TrimSpace(p.Model)
		masked = sciconfig.MaskKey(key)
	} else if pc, ok := cfg.Providers[cfg.Provider]; ok {
		key = strings.TrimSpace(pc.Key)
		masked = sciconfig.MaskKey(key)
	}
	if key == "" {
		return masked, "", fmt.Errorf("no API key in science config — configure a profile in Bridge")
	}
	tpl, ok := sciconfig.TemplateByID(templateID)
	if !ok {
		if spec, ok2 := proxy.LookupProvider(cfg.Provider); ok2 {
			adapter = spec.Name
			_ = os.Setenv(spec.KeyEnv, key)
			return masked, adapter, nil
		}
		return masked, "", fmt.Errorf("unknown provider template %q", templateID)
	}
	adapter = tpl.Adapter
	switch tpl.Adapter {
	case "deepseek":
		_ = os.Setenv("DEEPSEEK_API_KEY", key)
	case "qwen":
		_ = os.Setenv("DASHSCOPE_API_KEY", key)
	case "moonshot":
		_ = os.Setenv("MOONSHOT_API_KEY", key)
	case "zhipu":
		_ = os.Setenv("ZHIPU_API_KEY", key)
	default:
		_ = os.Setenv("OPENAI_API_KEY", key)
		if baseURL != "" {
			_ = os.Setenv("OPENAI_BASE_URL", baseURL)
		}
	}
	if model != "" {
		_ = os.Setenv("LUMEN_SCIENCE_MODEL", model)
	}
	return masked, adapter, nil
}

// providerStatus returns masked key info without requiring a valid key.
func providerStatus(cfg sciconfig.File) (masked, adapter string) {
	p := cfg.ActiveProfile()
	if p != nil {
		masked = sciconfig.MaskKey(strings.TrimSpace(p.APIKey))
		if tpl, ok := sciconfig.TemplateByID(p.TemplateID); ok {
			adapter = tpl.Adapter
		}
		return masked, adapter
	}
	if pc, ok := cfg.Providers[cfg.Provider]; ok {
		masked = sciconfig.MaskKey(strings.TrimSpace(pc.Key))
		adapter = cfg.Provider
	}
	return masked, adapter
}
