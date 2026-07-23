package runtime

import (
	"testing"

	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/proxy"
)

func TestResolveProfileOpenAICustom(t *testing.T) {
	p := sciconfig.Profile{
		ID: "p1", Name: "custom", TemplateID: "custom-openai",
		BaseURL: "https://api.example.com/v1",
		APIKey:  "sk-test",
		Model:   "foo-bar",
	}
	r, err := resolveProfile(p)
	if err != nil {
		t.Fatal(err)
	}
	if r.Spec.Mode != proxy.ModeOpenAI {
		t.Fatalf("mode %s", r.Spec.Mode)
	}
	if r.Spec.URL != "https://api.example.com/v1/chat/completions" {
		t.Fatalf("url %s", r.Spec.URL)
	}
}

func TestResolveProfileOpenAIResponses(t *testing.T) {
	p := sciconfig.Profile{
		ID: "p2", Name: "resp", TemplateID: "custom-openai-responses",
		BaseURL: "https://api.example.com/v1",
		APIKey:  "sk-test",
		Model:   "foo-bar",
	}
	r, err := resolveProfile(p)
	if err != nil {
		t.Fatal(err)
	}
	if r.Spec.Mode != proxy.ModeResponses {
		t.Fatalf("mode %s", r.Spec.Mode)
	}
	if r.Spec.URL != "https://api.example.com/v1/responses" {
		t.Fatalf("url %s", r.Spec.URL)
	}
}

func TestResolveProfileOpenAICustomRejectsAnthropicURL(t *testing.T) {
	p := sciconfig.Profile{
		ID: "p3", Name: "bad", TemplateID: "custom-openai",
		BaseURL: "https://api.moonshot.cn/anthropic",
		APIKey:  "sk-test",
		Model:   "kimi",
	}
	_, err := resolveProfile(p)
	if err == nil {
		t.Fatal("expected error for anthropic url")
	}
}

func TestResolveProfileKimiThinking(t *testing.T) {
	p := sciconfig.Profile{
		ID: "p4", Name: "kimi", TemplateID: "kimi",
		BaseURL: "https://api.moonshot.cn/anthropic",
		APIKey:  "sk-test",
		Model:   "kimi-k2.7-code",
	}
	r, err := resolveProfile(p)
	if err != nil {
		t.Fatal(err)
	}
	if r.Spec.ThinkingPolicy != "enabled" {
		t.Fatalf("policy %q", r.Spec.ThinkingPolicy)
	}
	if !r.Spec.ForceModelOverride {
		t.Fatal("force shell")
	}
}

func TestSpecFingerprintChangesWithFormat(t *testing.T) {
	a := ResolvedSpec{Adapter: "openai-custom", APIFormat: "openai_chat", BaseURL: "u", Model: "m"}
	b := ResolvedSpec{Adapter: "openai-custom", APIFormat: "openai_responses", BaseURL: "u", Model: "m"}
	if SpecFingerprint(a) == SpecFingerprint(b) {
		t.Fatal("fingerprint should differ by api_format")
	}
}
