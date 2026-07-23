package proxy

import "testing"

func TestNormalizeOpenAIBase(t *testing.T) {
	cases := []struct {
		in, want string
	}{
		{"https://api.example.com/v1", "https://api.example.com/v1"},
		{"https://api.example.com/v1/chat/completions", "https://api.example.com/v1"},
		{"https://api.example.com/v1/models", "https://api.example.com/v1"},
		{"https://api.example.com", "https://api.example.com/v1"},
		{"https://dashscope.aliyuncs.com/compatible-mode/v1/", "https://dashscope.aliyuncs.com/compatible-mode/v1"},
	}
	for _, c := range cases {
		got := NormalizeOpenAIBase(c.in)
		if got != c.want {
			t.Fatalf("NormalizeOpenAIBase(%q)=%q want %q", c.in, got, c.want)
		}
	}
}

func TestLooksLikeAnthropicBase(t *testing.T) {
	if !LooksLikeAnthropicBase("https://api.moonshot.cn/anthropic") {
		t.Fatal("expected anthropic detect")
	}
	if LooksLikeAnthropicBase("https://api.openai.com/v1") {
		t.Fatal("openai should not look anthropic")
	}
}

func TestOpenAIChatSpecForceShell(t *testing.T) {
	spec := OpenAIChatSpec("https://api.example.com/v1", "my-model")
	if spec.Mode != ModeOpenAI {
		t.Fatalf("mode %s", spec.Mode)
	}
	if !spec.ForceModelOverride || spec.ForceModel != "my-model" {
		t.Fatalf("force %+v", spec)
	}
	if ResolveModel(spec, "claude-opus-4-8") != "my-model" {
		t.Fatal("resolve")
	}
}

func TestRelaySpecForceShell(t *testing.T) {
	spec := RelaySpec("https://open.bigmodel.cn/api/anthropic", "k", "glm-5.2", nil)
	if !spec.ForceModelOverride {
		t.Fatal("expected force")
	}
	if len(spec.Models) != 1 || spec.Models[0].ID != "claude-opus-4-8" {
		t.Fatalf("models %+v", spec.Models)
	}
	if ResolveModel(spec, "claude-opus-4-8") != "glm-5.2" {
		t.Fatal("resolve force")
	}
}

func TestNormalizeThinkingKimi(t *testing.T) {
	tr := NormalizeThinking("relay", "enabled", "kimi-k2.7-code", true, "auto")
	if !tr.DropToolChoice {
		t.Fatal("expected drop tool_choice")
	}
	if tr.ThinkingJSON == "" || tr.ThinkingJSON == `{"type":"disabled"}` {
		t.Fatalf("thinking %s", tr.ThinkingJSON)
	}
	if !tr.FilterWebSearch {
		t.Fatal("filter web_search")
	}
}
