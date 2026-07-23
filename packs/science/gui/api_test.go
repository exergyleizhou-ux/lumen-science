package gui

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"lumen/internal/config"
	sciconfig "lumen/internal/science/config"
)

func testServer(t *testing.T) (*Server, string) {
	t.Helper()
	dir := t.TempDir()
	cfg := &config.File{}
	srv, err := New(Config{
		SciDir:   dir,
		LumenCfg: cfg,
		Addr:     "127.0.0.1:0",
		Version:  "test",
	})
	if err != nil {
		t.Fatal(err)
	}
	return srv, dir
}

func TestStaticIndexAndAssets(t *testing.T) {
	srv, _ := testServer(t)
	ts := httptest.NewServer(srv.mux)
	defer ts.Close()

	for _, path := range []string{"/", "/assets/app.css", "/assets/app.js", "/assets/fonts.css", "/assets/fonts/geist.woff2"} {
		resp, err := http.Get(ts.URL + path)
		if err != nil {
			t.Fatal(err)
		}
		if resp.StatusCode != http.StatusOK {
			t.Fatalf("%s status %d", path, resp.StatusCode)
		}
		resp.Body.Close()
	}
}

func TestAPIHealthAndVersion(t *testing.T) {
	srv, _ := testServer(t)
	ts := httptest.NewServer(securityHeaders(srv.cors(srv.wrapMiddleware(srv.mux))))
	defer ts.Close()

	resp, err := http.Get(ts.URL + "/api/health")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if resp.Header.Get("X-Content-Type-Options") != "nosniff" {
		t.Fatal("missing security header")
	}
	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}
	if body["status"] != "ok" || body["version"] != "test" {
		t.Fatalf("unexpected health: %v", body)
	}

	resp2, err := http.Get(ts.URL + "/api/version")
	if err != nil {
		t.Fatal(err)
	}
	defer resp2.Body.Close()
	var ver map[string]any
	_ = json.NewDecoder(resp2.Body).Decode(&ver)
	if ver["version"] != "test" {
		t.Fatalf("version: %v", ver)
	}
}

func TestAPIConfigGetPut(t *testing.T) {
	srv, dir := testServer(t)
	ts := httptest.NewServer(srv.cors(srv.wrapMiddleware(srv.mux)))
	defer ts.Close()

	resp, err := http.Get(ts.URL + "/api/config")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var cfg map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&cfg); err != nil {
		t.Fatal(err)
	}
	if cfg["provider"] != "deepseek" {
		t.Fatalf("provider: %v", cfg["provider"])
	}

	req, _ := http.NewRequest(http.MethodPut, ts.URL+"/api/config", strings.NewReader(`{"proxy_port":8765}`))
	req.Header.Set("Content-Type", "application/json")
	resp2, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp2.Body.Close()
	if resp2.StatusCode != http.StatusBadRequest {
		t.Fatalf("expected 400 for reserved port, got %d", resp2.StatusCode)
	}

	// valid update
	req3, _ := http.NewRequest(http.MethodPut, ts.URL+"/api/config", strings.NewReader(`{"proxy_port":19001,"sandbox_port":19002}`))
	req3.Header.Set("Content-Type", "application/json")
	resp3, err := http.DefaultClient.Do(req3)
	if err != nil {
		t.Fatal(err)
	}
	defer resp3.Body.Close()
	if resp3.StatusCode != http.StatusOK {
		t.Fatalf("update status %d", resp3.StatusCode)
	}
	cfgFile, err := sciconfig.Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfgFile.ProxyPort != 19001 || cfgFile.SandboxPort != 19002 {
		t.Fatalf("persisted ports: %+v", cfgFile)
	}

	req4, _ := http.NewRequest(http.MethodPut, ts.URL+"/api/config", strings.NewReader(`{"tooluse_shim":"detect"}`))
	req4.Header.Set("Content-Type", "application/json")
	resp4, err := http.DefaultClient.Do(req4)
	if err != nil {
		t.Fatal(err)
	}
	defer resp4.Body.Close()
	if resp4.StatusCode != http.StatusOK {
		t.Fatalf("tooluse_shim update %d", resp4.StatusCode)
	}
	cfgFile, err = sciconfig.Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfgFile.ToolUseShim != "detect" {
		t.Fatalf("tooluse_shim: %q", cfgFile.ToolUseShim)
	}
}

func TestHandleQuitProxy(t *testing.T) {
	srv, _ := testServer(t)
	ts := httptest.NewServer(srv.cors(srv.wrapMiddleware(srv.mux)))
	defer ts.Close()

	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/quit-proxy", strings.NewReader("{}"))
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("quit-proxy status %d", resp.StatusCode)
	}
	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}
	if body["ok"] != true {
		t.Fatalf("body %v", body)
	}
}

func TestAPIDoctorJSONTags(t *testing.T) {
	srv, _ := testServer(t)
	ts := httptest.NewServer(srv.cors(srv.wrapMiddleware(srv.mux)))
	defer ts.Close()

	resp, err := http.Get(ts.URL + "/api/doctor")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}
	results, _ := body["results"].([]any)
	if len(results) == 0 {
		t.Fatal("expected doctor results")
	}
	first, _ := results[0].(map[string]any)
	if _, ok := first["level"]; !ok {
		t.Fatalf("expected lowercase json tag level, got keys %v", first)
	}
}

func TestSharedManagerReuse(t *testing.T) {
	dir := t.TempDir()
	cfg := &config.File{}
	m1, err := panelManager(dir, cfg)
	if err != nil {
		t.Fatal(err)
	}
	m2, err := panelManager(dir, cfg)
	if err != nil {
		t.Fatal(err)
	}
	if m1 != m2 {
		t.Fatal("expected same manager instance in GUI process")
	}
	resetPanelManager()
}
