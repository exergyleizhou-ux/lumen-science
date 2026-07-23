package proxy

import (
	"bytes"
	"context"
	"encoding/json"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"lumen/internal/config"
)

func TestResolveModel(t *testing.T) {
	spec := BuiltInProviders["deepseek"]
	cases := []struct {
		in, want string
	}{
		{"", "deepseek-v4-flash"},
		{"claude-opus-4-8", "deepseek-v4-pro"},
		{"claude-haiku-4-5", "deepseek-v4-flash"},
		{"claude-sonnet-5", "deepseek-v4-flash"},
		{"claude-opus-4-820260701", "deepseek-v4-pro"},
	}
	for _, tc := range cases {
		if got := ResolveModel(spec, tc.in); got != tc.want {
			t.Errorf("ResolveModel(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}

func TestNormalizeAnthropicThinking(t *testing.T) {
	spec := BuiltInProviders["deepseek"]
	req := map[string]any{
		"model":       "claude-opus-4-8",
		"thinking":    map[string]any{"type": "auto"},
		"tool_choice": map[string]any{"type": "any"},
	}
	out := NormalizeAnthropicBody(spec, req)
	th, _ := out["thinking"].(map[string]any)
	if th["type"] != "disabled" {
		t.Fatalf("forcing tool_choice should disable thinking, got %#v", th)
	}

	req2 := map[string]any{
		"model":    "claude-opus-4-8",
		"thinking": map[string]any{"type": "auto"},
	}
	out2 := NormalizeAnthropicBody(spec, req2)
	th2, _ := out2["thinking"].(map[string]any)
	if th2["type"] != "adaptive" {
		t.Fatalf("auto thinking should become adaptive, got %#v", th2)
	}
}

func TestAnthropicToOpenAI_ToolUse(t *testing.T) {
	spec := BuiltInProviders["qwen"]
	req := map[string]any{
		"model": "claude-opus-4-8",
		"messages": []any{
			map[string]any{
				"role":    "user",
				"content": "hi",
			},
		},
		"tools": []any{
			map[string]any{
				"name":        "bash",
				"description": "run bash",
				"input_schema": map[string]any{
					"type": "object",
				},
			},
		},
		"tool_choice": map[string]any{"type": "auto"},
		"max_tokens":  1024,
	}
	out := AnthropicToOpenAI(spec, req)
	if out["model"] != "qwen3.7-max" {
		t.Fatalf("model = %v", out["model"])
	}
	if out["tool_choice"] != "auto" {
		t.Fatalf("tool_choice = %v", out["tool_choice"])
	}
}

func TestOpenAIToAnthropic(t *testing.T) {
	resp := map[string]any{
		"id": "chatcmpl-1",
		"choices": []any{
			map[string]any{
				"message": map[string]any{
					"content": "hello",
					"tool_calls": []any{
						map[string]any{
							"id": "call_1",
							"function": map[string]any{
								"name":      "bash",
								"arguments": `{"command":"echo hi"}`,
							},
						},
					},
				},
				"finish_reason": "tool_calls",
			},
		},
		"usage": map[string]any{
			"prompt_tokens":     10,
			"completion_tokens": 5,
		},
	}
	out := OpenAIToAnthropic(resp, "claude-opus-4-8")
	if out["stop_reason"] != "tool_use" {
		t.Fatalf("stop_reason = %v", out["stop_reason"])
	}
	blocks, _ := out["content"].([]map[string]any)
	if len(blocks) != 2 {
		t.Fatalf("blocks = %d", len(blocks))
	}
}

func TestServerHealthAndModels(t *testing.T) {
	srv := newTestServer(t, ModeAnthropic, nil)
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		srv.handleHTTP(w, r)
	}))
	defer ts.Close()

	resp, err := http.Get(ts.URL + "/secret/health")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("health status %d", resp.StatusCode)
	}

	resp2, err := http.Get(ts.URL + "/secret/v1/models")
	if err != nil {
		t.Fatal(err)
	}
	defer resp2.Body.Close()
	if resp2.StatusCode != http.StatusOK {
		t.Fatalf("models status %d", resp2.StatusCode)
	}
}

func TestServerAuthSecret(t *testing.T) {
	// /health is intentionally public (orchestrator probes). Secrets guard /v1/*.
	srv := newTestServer(t, ModeAnthropic, nil)
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		srv.handleHTTP(w, r)
	}))
	defer ts.Close()

	resp, err := http.Get(ts.URL + "/health")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("health should be public, got %d", resp.StatusCode)
	}

	resp2, err := http.Get(ts.URL + "/v1/models")
	if err != nil {
		t.Fatal(err)
	}
	resp2.Body.Close()
	if resp2.StatusCode != http.StatusForbidden {
		t.Fatalf("expected 403 on /v1/models without secret, got %d", resp2.StatusCode)
	}
}

func TestServerAnthropicPassthrough(t *testing.T) {
	var gotBody map[string]any
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("x-api-key") != "test-key" {
			t.Fatalf("missing upstream key")
		}
		if r.Header.Get("Authorization") != "" {
			t.Fatalf("inbound auth must not be forwarded")
		}
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"text","text":"ok"}]}`))
	}))
	defer upstream.Close()

	spec := BuiltInProviders["deepseek"]
	spec.URL = upstream.URL
	srv, err := New(Config{
		Provider:   spec,
		APIKey:     "test-key",
		Addr:       "127.0.0.1:0",
		AuthSecret: "",
	})
	if err != nil {
		t.Fatal(err)
	}

	body := `{"model":"claude-opus-4-8","messages":[{"role":"user","content":"hi"}],"max_tokens":999999}`
	req := httptest.NewRequest(http.MethodPost, "/v1/messages", strings.NewReader(body))
	req.Header.Set("Authorization", "Bearer inbound-oauth")
	rec := httptest.NewRecorder()
	srv.handleHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status %d body %s", rec.Code, rec.Body.String())
	}
	if gotBody["model"] != "deepseek-v4-pro" {
		t.Fatalf("upstream model = %v", gotBody["model"])
	}
	if int(gotBody["max_tokens"].(float64)) != 65536 {
		t.Fatalf("max_tokens = %v", gotBody["max_tokens"])
	}
}

func TestServerOpenAITranslate(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !strings.HasPrefix(r.Header.Get("Authorization"), "Bearer ") {
			t.Fatalf("missing bearer auth")
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"id":"c1","choices":[{"message":{"content":"translated"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":2}}`))
	}))
	defer upstream.Close()

	spec := BuiltInProviders["qwen"]
	spec.URL = upstream.URL
	srv, err := New(Config{
		Provider: spec,
		APIKey:   "test-key",
		Addr:     "127.0.0.1:0",
	})
	if err != nil {
		t.Fatal(err)
	}

	body := `{"model":"claude-opus-4-8","messages":[{"role":"user","content":"hi"}]}`
	req := httptest.NewRequest(http.MethodPost, "/v1/messages", strings.NewReader(body))
	rec := httptest.NewRecorder()
	srv.handleHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status %d body %s", rec.Code, rec.Body.String())
	}
	var out map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatal(err)
	}
	if out["stop_reason"] != "end_turn" {
		t.Fatalf("stop_reason = %v", out["stop_reason"])
	}
}

func TestBlockedConnect(t *testing.T) {
	srv := newTestServer(t, ModeAnthropic, nil)
	req := httptest.NewRequest(http.MethodConnect, "http://api.anthropic.com:443", nil)
	req.Host = "api.anthropic.com:443"
	rec := httptest.NewRecorder()
	srv.handleConnect(rec, req)
	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("connect status = %d", rec.Code)
	}
}

func TestListenOnlyLoopback(t *testing.T) {
	_, err := New(Config{
		Provider: BuiltInProviders["deepseek"],
		APIKey:   "k",
		Addr:     "0.0.0.0:18991",
	})
	if err == nil {
		t.Fatal("expected error for non-loopback bind")
	}
}

func TestGenerateAuthSecret(t *testing.T) {
	s, err := GenerateAuthSecret()
	if err != nil || len(s) != 32 {
		t.Fatalf("secret len=%d err=%v", len(s), err)
	}
}

func newTestServer(t *testing.T, mode Mode, upstream http.HandlerFunc) *Server {
	t.Helper()
	spec := BuiltInProviders["deepseek"]
	if mode == ModeOpenAI {
		spec = BuiltInProviders["qwen"]
	}
	srv, err := New(Config{
		Provider:   spec,
		APIKey:     "test-key",
		Addr:       "127.0.0.1:0",
		AuthSecret: "secret",
	})
	if err != nil {
		t.Fatal(err)
	}
	_ = upstream
	return srv
}

func TestTunnelCopy(t *testing.T) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()
	go func() {
		c, err := ln.Accept()
		if err != nil {
			return
		}
		defer c.Close()
		buf := make([]byte, 64)
		n, _ := c.Read(buf)
		_, _ = c.Write(buf[:n])
	}()

	client, err := net.Dial("tcp", ln.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	server, err := net.Dial("tcp", ln.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	_, _ = client.Write([]byte("ping"))
	go tunnel(client, server)
	// smoke: no panic
}

func TestReplayAsSSE(t *testing.T) {
	spec := BuiltInProviders["qwen"]
	srv, err := New(Config{Provider: spec, APIKey: "k", Addr: "127.0.0.1:0"})
	if err != nil {
		t.Fatal(err)
	}
	aresp := OpenAIToAnthropic(map[string]any{
		"id": "c1",
		"choices": []any{map[string]any{
			"message":       map[string]any{"content": "hi"},
			"finish_reason": "stop",
		}},
		"usage": map[string]any{"prompt_tokens": 1, "completion_tokens": 2},
	}, "claude-opus-4-8")
	rec := httptest.NewRecorder()
	srv.replayAsSSE(rec, aresp)
	if !strings.Contains(rec.Body.String(), "event: message_start") {
		t.Fatalf("missing SSE events: %s", rec.Body.String())
	}
}

func TestUpstreamPostJSON_Retry(t *testing.T) {
	attempts := 0
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts++
		if attempts < 2 {
			hj, ok := w.(http.Hijacker)
			if ok {
				conn, _, _ := hj.Hijack()
				conn.Close()
				return
			}
		}
		_, _ = w.Write([]byte(`{"ok":true}`))
	}))
	defer ts.Close()

	u := &Upstream{}
	data, _, code, err := u.PostJSON(context.Background(), ts.URL, []byte(`{}`), map[string]string{})
	if err != nil || code != 200 {
		t.Fatalf("err=%v code=%d", err, code)
	}
	if !bytes.Contains(data, []byte("ok")) {
		t.Fatalf("body=%s", data)
	}
}

func TestBuildConfigFromLumen(t *testing.T) {
	cfg := &config.File{
		Providers: []config.ProviderConfig{
			{Name: "deepseek", APIKey: "from-toml"},
		},
	}
	out, err := BuildConfig("deepseek", "", "127.0.0.1:18991", "sec", "", "", cfg)
	if err != nil {
		t.Fatal(err)
	}
	if out.APIKey != "from-toml" {
		t.Fatalf("key=%q", out.APIKey)
	}
}
