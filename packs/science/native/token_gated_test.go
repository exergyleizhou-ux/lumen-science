package native

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	sciconfig "lumen/internal/science/config"
)

func TestCheckAuthTokenGated(t *testing.T) {
	if err := CheckAuth("preview_schema", ""); err == nil {
		t.Fatal("preview_schema should require token")
	}
	if err := CheckAuth("list_algorithms", ""); err == nil {
		t.Fatal("list_algorithms should require token")
	}
	if err := CheckAuth("search_datasets", ""); err != nil {
		t.Fatalf("search_datasets should be anonymous: %v", err)
	}
	if err := CheckAuth("preview_schema", "tok"); err != nil {
		t.Fatalf("preview_schema with token: %v", err)
	}
}

func TestC2DListAlgorithmsFleetWithToken(t *testing.T) {
	mock := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer c2d-fleet-tok" {
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

	root := repoRootForTest(t)
	bin := filepath.Join(t.TempDir(), "lumen-mcp-c2d")
	cmd := exec.Command("go", "build", "-o", bin, "./cmd/lumen-mcp-c2d")
	cmd.Dir = root
	cmd.Env = append(os.Environ(), "CGO_ENABLED=0")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build c2d: %v\n%s", err, out)
	}
	t.Setenv("LUMEN_MCP_C2D", bin)

	dir := t.TempDir()
	cfg := sciconfig.Default()
	cfg.Oasis.BaseURL = mock.URL
	cfg.Oasis.APIToken = "c2d-fleet-tok"
	if err := sciconfig.Save(dir, cfg); err != nil {
		t.Fatal(err)
	}

	mgr, err := NewManagerFromDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer mgr.Close()
	if err := mgr.Connect("c2d"); err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	text, err := mgr.CallToolCtx(ctx, "c2d", "list_algorithms", map[string]any{"dataset_id": "ds1"})
	if err != nil {
		t.Fatalf("list_algorithms with mock token: %v", err)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(text), &payload); err != nil {
		t.Fatalf("unmarshal: %v raw=%s", err, text)
	}
	prov, ok := payload["provenance"].(map[string]any)
	if !ok || prov["source"] != "oasis-c2d" {
		t.Fatalf("missing oasis-c2d provenance: %s", text)
	}
	results, ok := payload["results"].(map[string]any)
	if !ok {
		t.Fatalf("missing results: %s", text)
	}
	algs, ok := results["algorithms"].([]any)
	if !ok || len(algs) == 0 {
		t.Fatalf("expected algorithms data: %s", text)
	}
}

func TestOasisPreviewSchemaFleetWithToken(t *testing.T) {
	mock := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer oasis-fleet-tok" {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		if r.URL.Path != "/api/v1/datasets/ds1/preview" {
			http.NotFound(w, r)
			return
		}
		w.Write([]byte(`{"code":0,"data":{"lines":["a,b,c","1,2,3"],"line_count":2,"dataset_sample_count":100,"truncated":true}}`))
	}))
	defer mock.Close()

	root := repoRootForTest(t)
	bin := filepath.Join(t.TempDir(), "lumen-mcp-oasis")
	cmd := exec.Command("go", "build", "-o", bin, "./cmd/lumen-mcp-oasis")
	cmd.Dir = root
	cmd.Env = append(os.Environ(), "CGO_ENABLED=0")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build oasis: %v\n%s", err, out)
	}
	t.Setenv("LUMEN_MCP_OASIS", bin)

	dir := t.TempDir()
	cfg := sciconfig.Default()
	cfg.Oasis.BaseURL = mock.URL
	cfg.Oasis.APIToken = "oasis-fleet-tok"
	if err := sciconfig.Save(dir, cfg); err != nil {
		t.Fatal(err)
	}

	mgr, err := NewManagerFromDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer mgr.Close()
	if err := mgr.Connect("oasis"); err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	text, err := mgr.CallToolCtx(ctx, "oasis", "preview_schema", map[string]any{"dataset_id": "ds1"})
	if err != nil {
		t.Fatalf("preview_schema with mock token: %v", err)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(text), &payload); err != nil {
		t.Fatalf("unmarshal: %v raw=%s", err, text)
	}
	prov, ok := payload["provenance"].(map[string]any)
	if !ok || prov["source"] != "oasis" {
		t.Fatalf("missing oasis provenance: %s", text)
	}
	results, ok := payload["results"].(map[string]any)
	if !ok || results["line_count"] == nil {
		t.Fatalf("expected preview data: %s", text)
	}
}

func repoRootForTest(t *testing.T) string {
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
