package proxy

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestIsBlockedHostAnthropic(t *testing.T) {
	for _, h := range []string{"claude.ai", "api.anthropic.com", "foo.claude.com", "sub.claude.ai"} {
		if !isBlockedHost(h) {
			t.Fatalf("%q should be blocked", h)
		}
	}
}

func TestIsBlockedHostAllowsOther(t *testing.T) {
	for _, h := range []string{"example.com", "deepseek.com", "localhost"} {
		if isBlockedHost(h) {
			t.Fatalf("%q should not be blocked", h)
		}
	}
}

func TestConnectReplySetsClose(t *testing.T) {
	rec := httptest.NewRecorder()
	connectReply(rec, http.StatusUnauthorized)
	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("code %d", rec.Code)
	}
	if rec.Header().Get("Connection") != "close" {
		t.Fatal("expected Connection: close")
	}
}
