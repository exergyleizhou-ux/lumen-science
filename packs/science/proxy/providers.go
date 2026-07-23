package proxy

import "strings"

// CSSwitchParity is the CSSwitch release whose gateway behavior we target.
// CSSwitch 0.4.x ships a Rust inference gateway; Lumen keeps a Go data plane
// with behavioral parity (models, force-shell, thinking/tool policy, Responses).
const CSSwitchParity = "0.4.1"

// Mode is how the proxy talks to the upstream API.
type Mode string

const (
	ModeAnthropic  Mode = "anthropic"  // native Anthropic wire — passthrough + remap
	ModeOpenAI     Mode = "openai"     // OpenAI Chat Completions — translate Anthropic↔OpenAI
	ModeResponses  Mode = "responses"  // OpenAI Responses API — translate Anthropic↔Responses
)

// ModelEntry is one model exposed to Claude Science's selector.
type ModelEntry struct {
	ID          string
	DisplayName string
}

// ProviderSpec is a built-in upstream preset for the Science proxy.
type ProviderSpec struct {
	Name         string
	Mode         Mode
	URL          string
	KeyEnv       string
	Models       []ModelEntry
	ModelMap     map[string]string
	ModelCaps    map[string]int
	DefaultCap   int
	DefaultModel string
	DsmlCapable  bool // DeepSeek may leak DSML tool calls as plain text
	DualAuth     bool // relay: send x-api-key + Authorization Bearer

	// ThinkingPolicy: "" | "enabled" | "adaptive" (CSSwitch-aligned).
	ThinkingPolicy string
	// ForceModelOverride: Science selector shows one claude- shell; outbound uses ForceModel.
	ForceModelOverride bool
	ForceModel         string
}

// BuiltInProviders maps CLI/config provider names to proxy presets.
var BuiltInProviders = map[string]ProviderSpec{
	"deepseek": {
		Name:        "deepseek",
		Mode:        ModeAnthropic,
		URL:         "https://api.deepseek.com/anthropic/v1/messages",
		KeyEnv:      "DEEPSEEK_API_KEY",
		DsmlCapable: true,
		Models: []ModelEntry{
			{ID: "claude-opus-4-8", DisplayName: "DeepSeek V4 Pro"},
			{ID: "claude-haiku-4-5", DisplayName: "DeepSeek V4 Flash"},
		},
		ModelMap: map[string]string{
			"claude-opus-4-8":   "deepseek-v4-pro",
			"claude-sonnet-5":   "deepseek-v4-flash",
			"claude-sonnet-4-6": "deepseek-v4-flash",
			"claude-haiku-4-5":  "deepseek-v4-flash",
		},
		ModelCaps: map[string]int{
			"deepseek-v4-pro":   65536,
			"deepseek-v4-flash": 32768,
		},
		DefaultCap:   8192,
		DefaultModel: "deepseek-v4-flash",
	},
	"moonshot": {
		Name:   "moonshot",
		Mode:   ModeOpenAI,
		URL:    "https://api.moonshot.cn/v1/chat/completions",
		KeyEnv: "MOONSHOT_API_KEY",
		Models: []ModelEntry{
			{ID: "kimi-k2", DisplayName: "Kimi K2"},
			{ID: "moonshot-v1", DisplayName: "Moonshot V1"},
		},
		ModelMap: map[string]string{
			"claude-opus-4-8":   "kimi-k2-0711-preview",
			"claude-sonnet-5":   "moonshot-v1-8k",
			"claude-sonnet-4-6": "moonshot-v1-8k",
			"claude-haiku-4-5":  "moonshot-v1-8k",
		},
		DefaultCap:   8192,
		DefaultModel: "moonshot-v1-8k",
	},
	"zhipu": {
		Name:   "zhipu",
		Mode:   ModeOpenAI,
		URL:    "https://open.bigmodel.cn/api/paas/v4/chat/completions",
		KeyEnv: "ZHIPU_API_KEY",
		Models: []ModelEntry{
			{ID: "glm-4", DisplayName: "GLM-4"},
			{ID: "glm-4-flash", DisplayName: "GLM-4 Flash"},
			{ID: "glm-4.5", DisplayName: "GLM-4.5"},
		},
		ModelMap: map[string]string{
			"claude-opus-4-8":   "glm-4.5",
			"claude-sonnet-5":   "glm-4",
			"claude-sonnet-4-6": "glm-4",
			"claude-haiku-4-5":  "glm-4-flash",
		},
		DefaultCap:   8192,
		DefaultModel: "glm-4",
	},
	"qwen": {
		// Model list aligned with CSSwitch 0.4 gateway QWEN_MODELS
		// (desktop/gateway/src/config.rs).
		Name:   "qwen",
		Mode:   ModeOpenAI,
		URL:    "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions",
		KeyEnv: "DASHSCOPE_API_KEY",
		Models: []ModelEntry{
			{ID: "qwen3.7-max", DisplayName: "Qwen 3.7 Max"},
			{ID: "qwen-plus-latest", DisplayName: "Qwen Plus Latest"},
			{ID: "qwen-turbo", DisplayName: "Qwen Turbo"},
		},
		ModelMap: map[string]string{
			// Science may still send Claude shells
			"claude-opus-4-8":   "qwen3.7-max",
			"claude-sonnet-5":   "qwen-plus-latest",
			"claude-sonnet-4-6": "qwen-plus-latest",
			"claude-haiku-4-5":  "qwen-turbo",
			// CSSwitch static ids (identity)
			"qwen3.7-max":       "qwen3.7-max",
			"qwen-plus-latest":  "qwen-plus-latest",
			"qwen-turbo":        "qwen-turbo",
			// legacy names from older Lumen/CSSwitch profiles
			"qwen-max":  "qwen3.7-max",
			"qwen-plus": "qwen-plus-latest",
		},
		ModelCaps: map[string]int{
			"qwen3.7-max":      8192,
			"qwen-plus-latest": 8192,
			"qwen-turbo":       8192,
			"qwen-max":         8192,
			"qwen-plus":        8192,
		},
		DefaultCap:   8192,
		DefaultModel: "qwen-plus-latest",
	},
	"minimax": {
		Name:           "minimax",
		Mode:           ModeAnthropic,
		URL:            "https://api.minimaxi.com/anthropic/v1/messages",
		KeyEnv:         "MINIMAX_API_KEY",
		ThinkingPolicy: ThinkingPolicyAdaptive,
		Models: []ModelEntry{
			{ID: "claude-opus-4-8", DisplayName: "MiniMax M3"},
			{ID: "claude-haiku-4-5", DisplayName: "MiniMax M1"},
		},
		ModelMap: map[string]string{
			"claude-opus-4-8":   "MiniMax-M3",
			"claude-sonnet-5":   "MiniMax-M3",
			"claude-sonnet-4-6": "MiniMax-M3",
			"claude-haiku-4-5":  "MiniMax-M1",
		},
		DefaultCap:   8192,
		DefaultModel: "MiniMax-M3",
	},
}

// LookupProvider returns a provider spec by name (case-insensitive).
func LookupProvider(name string) (ProviderSpec, bool) {
	p, ok := BuiltInProviders[strings.ToLower(strings.TrimSpace(name))]
	return p, ok
}
