package proxy

import "testing"

func TestMatchCapabilityRulesForceShell(t *testing.T) {
	spec := RelaySpec("https://example.com/anthropic", "k", "glm-5", nil)
	rules := MatchCapabilityRules(spec)
	found := false
	for _, r := range rules {
		if r.ID == "provider.relay.force-model-shell" {
			found = true
		}
	}
	if !found {
		t.Fatalf("expected force-model-shell rule, got %+v", rules)
	}
}

func TestMatchCapabilityRulesResponses(t *testing.T) {
	spec := OpenAIResponsesSpec("https://dashscope.aliyuncs.com/compatible-mode/v1", "qwen-max")
	rules := MatchCapabilityRules(spec)
	ids := map[string]bool{}
	for _, r := range rules {
		ids[r.ID] = true
	}
	if !ids["provider.openai.responses-path"] {
		t.Fatal("missing responses-path")
	}
	if !ids["provider.dashscope.responses-tools-cap"] {
		t.Fatal("missing dashscope cap")
	}
}
