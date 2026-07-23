package proxy

import (
	"bytes"
	"encoding/json"
	"testing"
)

func TestPatchAnthropicBodyRaw_PreservesMessagesBytes(t *testing.T) {
	raw := []byte(`{"model":"claude-opus-4-8","messages":[{"role":"user","content":[{"type":"text","text":"hi","cache_control":{"type":"ephemeral"}}]}],"max_tokens":999999,"thinking":{"type":"auto"}}`)
	spec := BuiltInProviders["deepseek"]
	out, err := PatchAnthropicBodyRaw(raw, spec, false)
	if err != nil {
		t.Fatal(err)
	}
	// messages array must be byte-identical (cache_control preserved)
	var orig, patched map[string]json.RawMessage
	json.Unmarshal(raw, &orig)
	json.Unmarshal(out, &patched)
	if !bytes.Equal(orig["messages"], patched["messages"]) {
		t.Fatalf("messages bytes changed:\norig=%s\npatched=%s", orig["messages"], patched["messages"])
	}
	var body map[string]any
	json.Unmarshal(out, &body)
	if body["model"] != "deepseek-v4-pro" {
		t.Fatalf("model=%v", body["model"])
	}
	th, _ := body["thinking"].(map[string]any)
	if th["type"] != "adaptive" {
		t.Fatalf("thinking=%v", th)
	}
	if int(body["max_tokens"].(float64)) != 65536 {
		t.Fatalf("max_tokens=%v", body["max_tokens"])
	}
}

func TestPatchAnthropicBodyRaw_NoThinkingValidJSON(t *testing.T) {
	raw := []byte(`{"model":"claude-opus-4-8","messages":[{"role":"user","content":"hi"}],"max_tokens":999999}`)
	spec := BuiltInProviders["deepseek"]
	out, err := PatchAnthropicBodyRaw(raw, spec, false)
	if err != nil {
		t.Fatal(err)
	}
	var body map[string]any
	if err := json.Unmarshal(out, &body); err != nil {
		t.Fatalf("invalid JSON: %v\n%s", err, out)
	}
	if body["model"] != "deepseek-v4-pro" {
		t.Fatalf("model=%v", body["model"])
	}
	if _, has := body["thinking"]; has {
		t.Fatalf("unexpected thinking field: %v", body["thinking"])
	}
}

func TestPatchAnthropicBodyRaw_CacheBoostSystem(t *testing.T) {
	raw := []byte(`{"model":"claude-opus-4-8","system":"you are helpful","messages":[{"role":"user","content":"hi"}],"max_tokens":1024}`)
	spec := BuiltInProviders["deepseek"]
	out, err := PatchAnthropicBodyRaw(raw, spec, true)
	if err != nil {
		t.Fatal(err)
	}
	var body map[string]any
	json.Unmarshal(out, &body)
	sys, ok := body["system"].([]any)
	if !ok || len(sys) == 0 {
		t.Fatalf("system=%v", body["system"])
	}
	block, _ := sys[0].(map[string]any)
	cc, _ := block["cache_control"].(map[string]any)
	if cc["type"] != "ephemeral" {
		t.Fatalf("cache_control=%v", cc)
	}
}

func TestStableMarshalKeyOrder(t *testing.T) {
	fields := map[string]json.RawMessage{
		"messages": json.RawMessage(`[]`),
		"model":    json.RawMessage(`"x"`),
	}
	out := stableMarshal(fields, anthropicKeyOrder)
	if !bytes.HasPrefix(out, []byte(`{"model"`)) {
		t.Fatalf("expected model first, got %s", out)
	}
}
