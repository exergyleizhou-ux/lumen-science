package config

import "strings"

// Template describes a built-in provider preset for profile creation.
type Template struct {
	ID                    string
	Name                  string
	Category              string
	APIFormat             string
	Adapter               string
	BaseURL               string
	BaseURLEditable       bool
	RequiresModelOverride bool
	BuiltinModels         []string
	WebsiteURL            string
	Icon                  string
	IconColor             string
	// ThinkingPolicy: "" | "enabled" | "adaptive" (CSSwitch-aligned).
	ThinkingPolicy string
}

var templates = []Template{
	{ID: "deepseek", Name: "DeepSeek", Category: "cn_official", APIFormat: "anthropic", Adapter: "deepseek",
		BaseURL: "https://api.deepseek.com/anthropic", WebsiteURL: "https://platform.deepseek.com",
		Icon: "deepseek", IconColor: "#1E88E5",
		BuiltinModels: []string{"claude-opus-4-8", "claude-haiku-4-5"}},
	{ID: "glm", Name: "智谱 GLM", Category: "cn_official", APIFormat: "anthropic", Adapter: "relay",
		BaseURL: "https://open.bigmodel.cn/api/anthropic", BaseURLEditable: true,
		WebsiteURL: "https://open.bigmodel.cn",
		Icon:       "glm", IconColor: "#2E6BE6",
		BuiltinModels: []string{"glm-4.6", "glm-5", "glm-4.5-air"}},
	{ID: "xiaomi", Name: "小米 MiMo", Category: "cn_official", APIFormat: "anthropic", Adapter: "relay",
		BaseURL: "https://api.xiaomimimo.com/anthropic", BaseURLEditable: true, RequiresModelOverride: true,
		WebsiteURL: "https://xiaomimimo.com", Icon: "xiaomi", IconColor: "#FF6900",
		BuiltinModels: []string{"mimo-v2.5-pro"}},
	{ID: "siliconflow", Name: "硅基流动", Category: "cn_official", APIFormat: "anthropic", Adapter: "relay",
		BaseURL: "https://api.siliconflow.cn", BaseURLEditable: true, RequiresModelOverride: true,
		WebsiteURL: "https://siliconflow.cn", Icon: "siliconflow", IconColor: "#7C3AED",
		BuiltinModels: []string{"deepseek-ai/DeepSeek-V3", "zai-org/GLM-5.2"}},
	{ID: "openrouter", Name: "OpenRouter", Category: "custom", APIFormat: "anthropic", Adapter: "relay",
		BaseURL: "https://openrouter.ai/api", BaseURLEditable: true, RequiresModelOverride: true,
		WebsiteURL: "https://openrouter.ai", Icon: "openrouter", IconColor: "#6467F2",
		BuiltinModels: []string{"anthropic/claude-sonnet-5", "anthropic/claude-opus-4.8", "anthropic/claude-opus-4.8-fast"}},
	{ID: "kimi", Name: "Kimi（Moonshot）", Category: "cn_official", APIFormat: "anthropic", Adapter: "relay",
		BaseURL: "https://api.moonshot.cn/anthropic", BaseURLEditable: true, RequiresModelOverride: true,
		WebsiteURL: "https://platform.moonshot.cn", Icon: "kimi", IconColor: "#16182F",
		ThinkingPolicy: "enabled",
		BuiltinModels:  []string{"kimi-k2.7-code", "kimi-k2.7-code-highspeed", "kimi-k2.6"}},
	{ID: "minimax", Name: "MiniMax", Category: "cn_official", APIFormat: "anthropic", Adapter: "relay",
		BaseURL: "https://api.minimaxi.com/anthropic", BaseURLEditable: true, RequiresModelOverride: true,
		WebsiteURL: "https://platform.minimaxi.com", Icon: "minimax", IconColor: "#E1341E",
		ThinkingPolicy: "adaptive",
		BuiltinModels:  []string{"MiniMax-M3", "MiniMax-M2.7", "MiniMax-M2.7-highspeed"}},
	{ID: "qwen", Name: "通义千问", Category: "cn_official", APIFormat: "openai_chat", Adapter: "qwen",
		BaseURL: "https://dashscope.aliyuncs.com/compatible-mode/v1", WebsiteURL: "https://dashscope.aliyun.com",
		Icon: "qwen", IconColor: "#615CED",
		// CSSwitch 0.4 QWEN_MODELS
		BuiltinModels: []string{"qwen3.7-max", "qwen-plus-latest", "qwen-turbo"}},
	{ID: "moonshot", Name: "Moonshot", Category: "cn_official", APIFormat: "openai_chat", Adapter: "moonshot",
		BaseURL: "https://api.moonshot.cn/v1", WebsiteURL: "https://www.moonshot.cn",
		Icon: "moonshot", IconColor: "#000000",
		BuiltinModels: []string{"kimi-k2", "moonshot-v1"}},
	{ID: "zhipu", Name: "智谱 (OpenAI)", Category: "cn_official", APIFormat: "openai_chat", Adapter: "zhipu",
		BaseURL: "https://open.bigmodel.cn/api/paas/v4", WebsiteURL: "https://open.bigmodel.cn",
		Icon: "zhipu", IconColor: "#2E6BE6",
		BuiltinModels: []string{"glm-4", "glm-4-flash"}},
	{ID: "custom-openai", Name: "自定义 OpenAI", Category: "custom", APIFormat: "openai_chat", Adapter: "openai-custom",
		BaseURLEditable: true, RequiresModelOverride: true, Icon: "custom", IconColor: "#2563EB"},
	{ID: "custom-openai-responses", Name: "自定义 OpenAI Responses", Category: "custom", APIFormat: "openai_responses", Adapter: "openai-responses",
		BaseURLEditable: true, RequiresModelOverride: true, Icon: "custom", IconColor: "#0F766E"},
	{ID: "custom", Name: "自定义 Anthropic", Category: "custom", APIFormat: "anthropic", Adapter: "relay",
		BaseURLEditable: true, RequiresModelOverride: true, Icon: "custom", IconColor: "#6B7280"},
}

// ListTemplates returns all built-in provider templates.
func ListTemplates() []Template {
	out := make([]Template, len(templates))
	copy(out, templates)
	return out
}

// ResolveProfileBaseURL returns the effective upstream base URL for a profile.
// Non-editable native presets ignore stored base_url overrides.
func ResolveProfileBaseURL(p Profile) string {
	tpl, ok := TemplateByID(p.TemplateID)
	if !ok {
		tpl, _ = TemplateByID("custom")
	}
	stored := strings.TrimSpace(p.BaseURL)
	if stored == "" {
		return tpl.BaseURL
	}
	if tpl.BaseURLEditable || tpl.ID == "custom" {
		return stored
	}
	return tpl.BaseURL
}

// TemplateByID looks up a template by id.
func TemplateByID(id string) (Template, bool) {
	for _, t := range templates {
		if t.ID == id {
			return t, true
		}
	}
	return Template{}, false
}

// TemplateIDForLegacySlot maps v1 provider slot names to template ids.
func TemplateIDForLegacySlot(slot string) string {
	switch strings.ToLower(slot) {
	case "deepseek":
		return "deepseek"
	case "qwen":
		return "qwen"
	case "moonshot":
		return "moonshot"
	case "zhipu":
		return "zhipu"
	case "relay-glm":
		return "glm"
	case "relay-xiaomi":
		return "xiaomi"
	case "relay-siliconflow":
		return "siliconflow"
	case "relay-openrouter":
		return "openrouter"
	default:
		return "custom"
	}
}
