package proxy

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

const dsmlP2 = "｜｜"

func dsmlLeakBlock() string {
	return "<" + dsmlP2 + "DSML" + dsmlP2 + "tool_calls> <" + dsmlP2 + "DSML" + dsmlP2 + `invoke name="web_search"><` + dsmlP2 + "DSML" + dsmlP2 + `parameter name="query" string="true">geo</` + dsmlP2 + "DSML" + dsmlP2 + "parameter></" + dsmlP2 + "DSML" + dsmlP2 + "invoke> </" + dsmlP2 + "DSML" + dsmlP2 + "tool_calls>"
}

func startTestProxyE2E(t *testing.T, shim ToolUseShimMode, upstream http.HandlerFunc) (addr, secret string) {
	t.Helper()
	up := httptest.NewServer(upstream)
	t.Cleanup(up.Close)
	spec := BuiltInProviders["deepseek"]
	spec.URL = up.URL
	secret = "e2esecret"
	addr, _ = startProxyServer(t, Config{
		Provider: spec, APIKey: "sk-test",
		AuthSecret: secret, ToolUseShim: shim,
	})
	return addr, secret
}

func TestDSMLE2ERewriteNonStream(t *testing.T) {
	up := func(w http.ResponseWriter, r *http.Request) {
		body := map[string]any{
			"content":     []any{map[string]any{"type": "text", "text": dsmlLeakBlock()}},
			"stop_reason": "end_turn",
		}
		_ = json.NewEncoder(w).Encode(body)
	}
	addr, secret := startTestProxyE2E(t, ShimRewrite, up)
	reqBody := []byte(`{"model":"claude-opus-4-8","max_tokens":64,"stream":false,"tools":[{"name":"web_search","input_schema":{"type":"object","properties":{"query":{"type":"string"}}}}],"messages":[{"role":"user","content":"hi"}]}`)
	resp, err := http.Post("http://"+addr+"/"+secret+"/v1/messages", "application/json", bytes.NewReader(reqBody))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		t.Fatalf("status %d", resp.StatusCode)
	}
	var out map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		t.Fatal(err)
	}
	content, _ := out["content"].([]any)
	b0, _ := content[0].(map[string]any)
	if b0["type"] != "tool_use" {
		t.Fatalf("expected tool_use, got %#v", b0)
	}
	if out["stop_reason"] != "tool_use" {
		t.Fatalf("stop_reason %v", out["stop_reason"])
	}
}

func TestDSMLE2ECleanBodyNoToolUse(t *testing.T) {
	raw := []byte(`{"content":[{"type":"text","text":"hello"}],"stop_reason":"end_turn"}`)
	up := func(w http.ResponseWriter, r *http.Request) {
		w.Write(raw)
	}
	addr, secret := startTestProxyE2E(t, ShimRewrite, up)
	reqBody := []byte(`{"model":"claude-opus-4-8","max_tokens":1,"stream":false,"messages":[{"role":"user","content":"hi"}]}`)
	resp, err := http.Post("http://"+addr+"/"+secret+"/v1/messages", "application/json", bytes.NewReader(reqBody))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	got, _ := io.ReadAll(resp.Body)
	if !bytes.Contains(got, []byte("hello")) {
		t.Fatalf("body %s", got)
	}
	if bytes.Contains(got, []byte("tool_use")) {
		t.Fatal("unexpected rewrite on clean body")
	}
}

func TestDSMLE2EDetectDoesNotRewrite(t *testing.T) {
	up := func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`{"content":[{"type":"text","text":"` + dsmlLeakBlock() + `"}],"stop_reason":"end_turn"}`))
	}
	addr, secret := startTestProxyE2E(t, ShimDetect, up)
	reqBody := []byte(`{"model":"claude-opus-4-8","max_tokens":64,"stream":false,"tools":[{"name":"web_search","input_schema":{"type":"object","properties":{"query":{"type":"string"}}}}],"messages":[{"role":"user","content":"hi"}]}`)
	resp, err := http.Post("http://"+addr+"/"+secret+"/v1/messages", "application/json", bytes.NewReader(reqBody))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	got, _ := io.ReadAll(resp.Body)
	if !strings.Contains(string(got), "DSML") {
		t.Fatal("detect should pass through DSML text")
	}
	if strings.Contains(string(got), `"type":"tool_use"`) {
		t.Fatal("detect must not rewrite")
	}
}

func TestDSMLE2EIllegalBooleanVoided(t *testing.T) {
	blk := "<" + dsmlP2 + "DSML" + dsmlP2 + `tool_calls> <` + dsmlP2 + "DSML" + dsmlP2 + `invoke name="setflag"><` + dsmlP2 + "DSML" + dsmlP2 + `parameter name="flag">maybe</` + dsmlP2 + "DSML" + dsmlP2 + `parameter></` + dsmlP2 + "DSML" + dsmlP2 + `invoke> </` + dsmlP2 + "DSML" + dsmlP2 + `tool_calls>`
	up := func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{
			"content": []any{map[string]any{"type": "text", "text": blk}}, "stop_reason": "end_turn",
		})
	}
	addr, secret := startTestProxyE2E(t, ShimRewrite, up)
	reqBody := []byte(`{"model":"claude-opus-4-8","max_tokens":64,"stream":false,"tools":[{"name":"setflag","input_schema":{"type":"object","properties":{"flag":{"type":"boolean"}},"required":["flag"]}}],"messages":[{"role":"user","content":"hi"}]}`)
	resp, err := http.Post("http://"+addr+"/"+secret+"/v1/messages", "application/json", bytes.NewReader(reqBody))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	got, _ := io.ReadAll(resp.Body)
	if strings.Contains(string(got), "tool_use") {
		t.Fatal("illegal boolean must not synthesize tool_use")
	}
}
