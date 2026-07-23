package gui

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"lumen/internal/science/config"
)

func repoRoot(t *testing.T) string {
	t.Helper()
	wd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 6; i++ {
		if _, err := os.Stat(filepath.Join(wd, "go.mod")); err == nil {
			return wd
		}
		wd = filepath.Dir(wd)
	}
	t.Fatal("go.mod not found")
	return ""
}

func buildMCPBinary(t *testing.T, name string) string {
	t.Helper()
	root := repoRoot(t)
	out := filepath.Join(t.TempDir(), name)
	cmd := exec.Command("go", "build", "-o", out, "./cmd/"+name)
	cmd.Dir = root
	cmd.Env = append(os.Environ(), "CGO_ENABLED=0")
	if outBytes, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build %s: %v\n%s", name, err, outBytes)
	}
	return out
}

func TestHandleNativeC2DAlgorithmsNeedsLogin(t *testing.T) {
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

	body, _ := json.Marshal(map[string]string{"dataset_id": "ds_test"})
	req := httptest.NewRequest(http.MethodPost, "/api/native/c2d/algorithms", bytes.NewReader(body))
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status %d", rec.Code)
	}
	var resp map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	if resp["ok"] != false || resp["need_login"] != true {
		t.Fatalf("resp: %v", resp)
	}
}

func TestHandleNativeC2DAlgorithmsWithToken(t *testing.T) {
	mock := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer c2d-tok" {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		if r.URL.Path != "/api/v1/compute/algorithms" {
			http.NotFound(w, r)
			return
		}
		w.Write([]byte(`{"code":0,"data":{"algorithms":[{"id":"algo1","name":"Causal DAG"}]}}`))
	}))
	defer mock.Close()

	t.Setenv("LUMEN_MCP_C2D", buildMCPBinary(t, "lumen-mcp-c2d"))

	dir := t.TempDir()
	cfg := config.Default()
	cfg.Oasis.BaseURL = mock.URL
	cfg.Oasis.APIToken = "c2d-tok"
	if err := config.Save(dir, cfg); err != nil {
		t.Fatal(err)
	}
	api := NewAPI(dir, nil, "test", time.Now())
	mux := http.NewServeMux()
	api.RegisterNative(mux)

	body, _ := json.Marshal(map[string]string{"dataset_id": "ds_live"})
	req := httptest.NewRequest(http.MethodPost, "/api/native/c2d/algorithms", bytes.NewReader(body))
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status %d %s", rec.Code, rec.Body.String())
	}
	var resp map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	if resp["ok"] != true {
		t.Fatalf("resp: %v", resp)
	}
	data, ok := resp["data"].(map[string]any)
	if !ok || data["provenance"] == nil {
		t.Fatalf("data missing provenance: %v", resp["data"])
	}
}

func TestHandleOasisProbeNeedsLogin(t *testing.T) {
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

	body, _ := json.Marshal(map[string]string{"dataset_id": "ds_test"})
	req := httptest.NewRequest(http.MethodPost, "/api/oasis/probe", bytes.NewReader(body))
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status %d", rec.Code)
	}
	var resp map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	if resp["need_login"] != true {
		t.Fatalf("resp: %v", resp)
	}
}

func TestHandleOasisProbeWithToken(t *testing.T) {
	mock := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer probe-tok" {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		w.Write([]byte(`{"code":0,"data":{"lines":["a,b"],"line_count":1,"dataset_sample_count":10,"truncated":false}}`))
	}))
	defer mock.Close()

	dir := t.TempDir()
	cfg := config.Default()
	cfg.Oasis.BaseURL = mock.URL
	cfg.Oasis.APIToken = "probe-tok"
	if err := config.Save(dir, cfg); err != nil {
		t.Fatal(err)
	}
	api := NewAPI(dir, nil, "test", time.Now())
	mux := http.NewServeMux()
	api.RegisterNative(mux)

	body, _ := json.Marshal(map[string]string{"dataset_id": "ds_live"})
	req := httptest.NewRequest(http.MethodPost, "/api/oasis/probe", bytes.NewReader(body))
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status %d %s", rec.Code, rec.Body.String())
	}
	var resp map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	if resp["ok"] != true {
		t.Fatalf("resp: %v", resp)
	}
	preview, ok := resp["preview"].(map[string]any)
	if !ok || preview["line_count"] == nil {
		t.Fatalf("preview: %v", resp["preview"])
	}
}

func TestHandleNativeStatusListsFleet(t *testing.T) {
	t.Setenv("LUMEN_MCP_PUBMED", buildMCPBinary(t, "lumen-mcp-pubmed"))
	t.Setenv("LUMEN_MCP_OASIS", buildMCPBinary(t, "lumen-mcp-oasis"))
	t.Setenv("LUMEN_MCP_CHEMBL", buildMCPBinary(t, "lumen-mcp-chembl"))
	t.Setenv("LUMEN_MCP_C2D", buildMCPBinary(t, "lumen-mcp-c2d"))
	t.Setenv("LUMEN_MCP_GEO", buildMCPBinary(t, "lumen-mcp-geo"))

	dir := t.TempDir()
	if err := config.Save(dir, config.Default()); err != nil {
		t.Fatal(err)
	}
	api := NewAPI(dir, nil, "test", time.Now())
	mux := http.NewServeMux()
	api.RegisterNative(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/native/status", nil)
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status %d", rec.Code)
	}
	var resp map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	fleet, ok := resp["fleet"].([]any)
	if !ok || len(fleet) < 5 {
		t.Fatalf("fleet len: %v", resp["fleet"])
	}
}
