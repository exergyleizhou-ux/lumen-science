package proxy

import (
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestConnectE2EAnthropicHost401(t *testing.T) {
	spec := BuiltInProviders["deepseek"]
	addr, _ := startProxyServer(t, Config{Provider: spec, APIKey: "sk-test"})
	conn, err := net.DialTimeout("tcp", addr, 2*time.Second)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	_, err = conn.Write([]byte("CONNECT claude.ai:443 HTTP/1.1\r\nHost: claude.ai:443\r\n\r\n"))
	if err != nil {
		t.Fatal(err)
	}
	buf := make([]byte, 4096)
	n, err := conn.Read(buf)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(buf[:n]), "401") {
		t.Fatalf("expected 401 fast-fail, got %q", buf[:n])
	}
}

func TestConnectE2EClaudeCom401(t *testing.T) {
	srv := newTestServer(t, ModeAnthropic, nil)
	req := httptest.NewRequest(http.MethodConnect, "http://api.claude.com:443", nil)
	req.Host = "api.claude.com:443"
	rec := httptest.NewRecorder()
	srv.handleConnect(rec, req)
	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("want 401, got %d", rec.Code)
	}
}

func TestAuthE2EForbiddenWithoutSecret(t *testing.T) {
	// /health stays public; path-secret guards model/messages endpoints.
	spec := BuiltInProviders["deepseek"]
	addr, _ := startProxyServer(t, Config{
		Provider: spec, APIKey: "sk-test", AuthSecret: "s3cret",
	})
	resp, err := http.Get("http://" + addr + "/health")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("health should be public, got %d", resp.StatusCode)
	}
	resp2, err := http.Get("http://" + addr + "/v1/models")
	if err != nil {
		t.Fatal(err)
	}
	defer resp2.Body.Close()
	if resp2.StatusCode != http.StatusForbidden {
		t.Fatalf("want 403 on /v1/models without secret, got %d", resp2.StatusCode)
	}
}

func TestMalformedBodyE2E400(t *testing.T) {
	spec := BuiltInProviders["deepseek"]
	secret := "sec"
	addr, _ := startProxyServer(t, Config{
		Provider: spec, APIKey: "sk-test", AuthSecret: secret,
	})
	resp, err := http.Post("http://"+addr+"/"+secret+"/v1/messages", "application/json", strings.NewReader(`{"messages":"not-array"}`))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("want 400, got %d", resp.StatusCode)
	}
}
