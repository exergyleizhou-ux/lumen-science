package proxy

import "testing"

func TestCSSwitchParityConstant(t *testing.T) {
	if CSSwitchParity != "0.4.1" {
		t.Fatalf("CSSwitchParity=%q", CSSwitchParity)
	}
}

func TestQwenModelsMatchCSSwitch04(t *testing.T) {
	spec, ok := LookupProvider("qwen")
	if !ok {
		t.Fatal("qwen missing")
	}
	if len(spec.Models) < 3 {
		t.Fatalf("models %+v", spec.Models)
	}
	if spec.Models[0].ID != "qwen3.7-max" || spec.Models[1].ID != "qwen-plus-latest" {
		t.Fatalf("CSSwitch QWEN_MODELS mismatch: %+v", spec.Models)
	}
	if ResolveModel(spec, "claude-opus-4-8") != "qwen3.7-max" {
		t.Fatalf("shell map %s", ResolveModel(spec, "claude-opus-4-8"))
	}
	if ResolveModel(spec, "qwen-max") != "qwen3.7-max" {
		t.Fatalf("legacy qwen-max map %s", ResolveModel(spec, "qwen-max"))
	}
}

func TestForceShellDisplayNameIsRealModel(t *testing.T) {
	spec := RelaySpec("https://example.com/anthropic", "k", "glm-5.2", nil)
	if !spec.ForceModelOverride || spec.ForceModel != "glm-5.2" {
		t.Fatalf("%+v", spec)
	}
	// handleModels uses ForceModel as display_name (CSSwitch force_shell_response)
	if spec.Models[0].DisplayName != "glm-5.2" && spec.ForceModel != "glm-5.2" {
		t.Fatal("expected force model identity")
	}
}
