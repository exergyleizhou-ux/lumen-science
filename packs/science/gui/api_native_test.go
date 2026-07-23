package gui

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"lumen/internal/science/config"
)

func TestHandleOasisTokenRoundTrip(t *testing.T) {
	dir := t.TempDir()
	if err := os.MkdirAll(dir, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := config.Save(dir, config.Default()); err != nil {
		t.Fatal(err)
	}
	api := NewAPI(dir, nil, "test", time.Now())
	mux := http.NewServeMux()
	api.RegisterNative(mux)

	body, _ := json.Marshal(map[string]string{"access_token": "test-access-token-xyz"})
	req := httptest.NewRequest(http.MethodPost, "/api/oasis/token", bytes.NewReader(body))
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("POST token: %d %s", rec.Code, rec.Body.String())
	}

	req2 := httptest.NewRequest(http.MethodGet, "/api/oasis", nil)
	rec2 := httptest.NewRecorder()
	mux.ServeHTTP(rec2, req2)
	if rec2.Code != http.StatusOK {
		t.Fatalf("GET oasis: %d", rec2.Code)
	}
	var resp map[string]any
	if err := json.Unmarshal(rec2.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	if resp["token_set"] != true {
		t.Fatalf("token_set: %v", resp["token_set"])
	}

	cfg, err := config.Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.OasisToken() != "test-access-token-xyz" {
		t.Fatalf("stored token mismatch")
	}
	path := filepath.Join(dir, "config.json")
	if st, err := os.Stat(path); err != nil || st.Mode().Perm() != 0o600 {
		t.Fatalf("config perms: %v", err)
	}
}
