package proxy

import "strings"

// CapabilityRule is a diagnostic rule id (CSSwitch capabilities.v1 style, slim).
type CapabilityRule struct {
	ID     string `json:"id"`
	Status string `json:"status"` // supported | limited | unsupported
	Action string `json:"action"` // normalize | degrade | block
	Reason string `json:"reason"`
}

// MatchCapabilityRules returns rules that apply to the active spec (observability).
func MatchCapabilityRules(spec ProviderSpec) []CapabilityRule {
	var out []CapabilityRule
	if spec.ForceModelOverride {
		out = append(out, CapabilityRule{
			ID:     "provider.relay.force-model-shell",
			Status: "supported",
			Action: "normalize",
			Reason: "Science selector sees claude-opus shell; outbound model forced to profile model",
		})
	}
	if spec.ThinkingPolicy == ThinkingPolicyEnabled || strings.Contains(strings.ToLower(spec.ForceModel), "kimi") {
		out = append(out, CapabilityRule{
			ID:     "provider.kimi.relay-thinking-enabled",
			Status: "limited",
			Action: "degrade",
			Reason: "Kimi requires thinking enabled; forced tool_choice is dropped",
		})
		out = append(out, CapabilityRule{
			ID:     "tool.kimi.web_search.server-tool-filter",
			Status: "limited",
			Action: "degrade",
			Reason: "web_search tool stripped before upstream to avoid server_tool_use blocks",
		})
	}
	if spec.Mode == ModeResponses {
		out = append(out, CapabilityRule{
			ID:     "provider.openai.responses-path",
			Status: "supported",
			Action: "normalize",
			Reason: "Anthropic Messages translated to OpenAI Responses; non-stream upstream + local SSE replay",
		})
		if strings.Contains(spec.URL, "dashscope.aliyuncs.com") {
			out = append(out, CapabilityRule{
				ID:     "provider.dashscope.responses-tools-cap",
				Status: "limited",
				Action: "degrade",
				Reason: "DashScope Responses tool requests cap max_output_tokens at 8192",
			})
		}
	}
	if spec.Mode == ModeOpenAI {
		out = append(out, CapabilityRule{
			ID:     "provider.openai.chat-completions",
			Status: "supported",
			Action: "normalize",
			Reason: "Anthropic Messages translated to OpenAI Chat Completions",
		})
	}
	if spec.DsmlCapable {
		out = append(out, CapabilityRule{
			ID:     "provider.deepseek.dsml-shim",
			Status: "supported",
			Action: "normalize",
			Reason: "DSML tool-use leak shim available (off|detect|rewrite)",
		})
	}
	return out
}
