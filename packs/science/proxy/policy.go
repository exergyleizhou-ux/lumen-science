package proxy

import (
	"strings"
)

// Thinking policy constants (aligned with CSSwitch template thinking_policy).
const (
	ThinkingPolicyNone     = ""
	ThinkingPolicyEnabled  = "enabled"  // Kimi: must enable; drop forced tool_choice
	ThinkingPolicyAdaptive = "adaptive" // MiniMax-style
)

// ThinkingResult is the outcome of NormalizeThinking for patch/translate callers.
type ThinkingResult struct {
	// ThinkingJSON is raw JSON for the thinking field, or empty to delete the field.
	ThinkingJSON string
	// DropToolChoice means remove tool_choice from the request (Kimi enabled + forced).
	DropToolChoice bool
	// FilterWebSearch drops tools named web_search for Kimi server-tool conflict.
	FilterWebSearch bool
	// RuleIDs for capability catalog / diagnostics.
	RuleIDs []string
}

// NormalizeThinking applies CSSwitch-aligned thinking / tool_choice rules.
// providerName is spec.Name; policy is spec.ThinkingPolicy; targetModel is resolved upstream model.
func NormalizeThinking(providerName, policy, targetModel string, forcingToolChoice bool, thinkingType string) ThinkingResult {
	var out ThinkingResult
	pol := strings.ToLower(strings.TrimSpace(policy))
	name := strings.ToLower(strings.TrimSpace(providerName))
	modelLower := strings.ToLower(targetModel)

	// Infer enabled policy for kimi models when template forgot to set it.
	if pol == "" && (strings.Contains(modelLower, "kimi") || name == "moonshot") {
		pol = ThinkingPolicyEnabled
	}

	switch {
	case pol == ThinkingPolicyEnabled || (name == "relay" && strings.Contains(modelLower, "kimi")):
		out.RuleIDs = append(out.RuleIDs, "provider.kimi.relay-thinking-enabled")
		if forcingToolChoice {
			out.DropToolChoice = true
		}
		// Always enable thinking with a small budget.
		out.ThinkingJSON = `{"type":"enabled","budget_tokens":1024}`
		out.FilterWebSearch = true
		return out

	case name == "minimax" || pol == ThinkingPolicyAdaptive:
		if forcingToolChoice {
			// MiniMax accepts adaptive + tools; leave thinking as-is if present.
			if thinkingType == "" || thinkingType == "auto" {
				out.ThinkingJSON = `{"type":"adaptive"}`
			}
			return out
		}
		if thinkingType == "auto" {
			out.ThinkingJSON = `{"type":"adaptive"}`
		}
		return out

	default:
		// deepseek / default: forced tool_choice disables thinking
		if forcingToolChoice {
			if name == "deepseek" {
				out.RuleIDs = append(out.RuleIDs, "tool.deepseek.forced-tool-choice-disable-thinking")
			}
			out.ThinkingJSON = `{"type":"disabled"}`
			return out
		}
		if thinkingType == "auto" {
			out.ThinkingJSON = `{"type":"adaptive"}`
		}
		return out
	}
}

// ShouldFilterKimiWebSearch reports whether to strip web_search from upstream tools.
func ShouldFilterKimiWebSearch(spec ProviderSpec, targetModel string) bool {
	if strings.EqualFold(spec.Name, "moonshot") {
		return true
	}
	if spec.ThinkingPolicy == ThinkingPolicyEnabled {
		return true
	}
	return strings.Contains(strings.ToLower(targetModel), "kimi")
}
