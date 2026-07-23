package config

import "testing"

func TestTemplateByIDDeepseek(t *testing.T) {
	tpl, ok := TemplateByID("deepseek")
	if !ok || tpl.Adapter != "deepseek" {
		t.Fatalf("%+v", tpl)
	}
}

func TestTemplateByIDCustom(t *testing.T) {
	tpl, ok := TemplateByID("custom")
	if !ok || !tpl.BaseURLEditable {
		t.Fatal()
	}
}

func TestListTemplatesCount(t *testing.T) {
	if len(ListTemplates()) < 7 {
		t.Fatalf("want >=7 templates")
	}
}

func TestRelayPresetsBaseURLEditable(t *testing.T) {
	for _, id := range []string{"glm", "xiaomi", "siliconflow", "openrouter", "custom"} {
		tpl, ok := TemplateByID(id)
		if !ok || !tpl.BaseURLEditable {
			t.Fatalf("%s: %+v", id, tpl)
		}
	}
}

func TestNativePresetsBaseURLReadOnly(t *testing.T) {
	for _, id := range []string{"deepseek", "qwen", "moonshot", "zhipu"} {
		tpl, ok := TemplateByID(id)
		if !ok || tpl.BaseURLEditable {
			t.Fatalf("%s: %+v", id, tpl)
		}
	}
}

func TestResolveProfileBaseURLXiaomiTokenPlan(t *testing.T) {
	p := Profile{
		TemplateID: "xiaomi",
		BaseURL:    "https://token-plan-cn.xiaomimimo.com/anthropic",
	}
	if got := ResolveProfileBaseURL(p); got != p.BaseURL {
		t.Fatalf("got %q", got)
	}
}

func TestResolveProfileBaseURLIgnoresDeepseekOverride(t *testing.T) {
	tpl, _ := TemplateByID("deepseek")
	p := Profile{TemplateID: "deepseek", BaseURL: "https://evil.example.com"}
	if got := ResolveProfileBaseURL(p); got != tpl.BaseURL {
		t.Fatalf("got %q want %q", got, tpl.BaseURL)
	}
}

func TestTemplateIDForLegacySlot(t *testing.T) {
	cases := map[string]string{
		"deepseek": "deepseek", "relay-glm": "glm", "unknown": "custom",
	}
	for in, want := range cases {
		if got := TemplateIDForLegacySlot(in); got != want {
			t.Fatalf("%s: %s", in, got)
		}
	}
}
