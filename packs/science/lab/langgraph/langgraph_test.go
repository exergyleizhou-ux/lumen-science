package langgraph

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

// enableLocalSidecar sets LUMEN_LANGGRAPH env for this test only.
// Prefers repo scripts/science/langgraph_runner.py, then ~/.lumen copy.
// Returns false (and skips via caller) when venv cannot import langgraph.
func enableLocalSidecar(t *testing.T) bool {
	t.Helper()
	home := os.Getenv("HOME")
	if home == "" {
		return false
	}
	venv := filepath.Join(home, ".lumen", "langgraph-venv")
	python := filepath.Join(venv, "bin", "python3")
	if st, err := os.Stat(python); err != nil || st.IsDir() {
		return false
	}

	script := ""
	if _, thisFile, _, ok := runtime.Caller(0); ok {
		// .../internal/science/lab/langgraph/langgraph_test.go → repo root
		repo := filepath.Clean(filepath.Join(filepath.Dir(thisFile), "..", "..", "..", ".."))
		cand := filepath.Join(repo, "scripts", "science", "langgraph_runner.py")
		if st, err := os.Stat(cand); err == nil && !st.IsDir() {
			script = cand
		}
	}
	if script == "" {
		script = filepath.Join(home, ".lumen", "langgraph_runner.py")
		if st, err := os.Stat(script); err != nil || st.IsDir() {
			return false
		}
	}

	t.Setenv("LUMEN_LANGGRAPH", "1")
	t.Setenv("LUMEN_LANGGRAPH_VENV", venv)
	t.Setenv("LUMEN_LANGGRAPH_SCRIPT", script)
	return IsAvailable()
}

func TestHealthHintNonEmpty(t *testing.T) {
	t.Setenv("LUMEN_LANGGRAPH", "")
	h := Health()
	if h["available"] != false {
		t.Fatalf("expected available=false without LUMEN_LANGGRAPH=1: %v", h)
	}
	hint, _ := h["hint"].(string)
	if strings.TrimSpace(hint) == "" {
		t.Fatal("health hint must be non-empty")
	}
}

func TestRunUnavailableClearError(t *testing.T) {
	t.Setenv("LUMEN_LANGGRAPH", "0")
	resp := Run(context.Background(), RunRequest{ProjectID: "t", Prompt: "hi"})
	if resp.OK {
		t.Fatal("expected ok=false when langgraph disabled")
	}
	if strings.TrimSpace(resp.Error) == "" {
		t.Fatal("expected non-empty error when unavailable")
	}
}

func TestRunEmptyPrompt(t *testing.T) {
	if !enableLocalSidecar(t) {
		t.Skip("langgraph venv not available")
	}
	resp := Run(context.Background(), RunRequest{ProjectID: "t", Prompt: "   "})
	if resp.OK || !strings.Contains(resp.Error, "prompt") {
		t.Fatalf("expected empty-prompt error, got %+v", resp)
	}
}

func TestRunIntegration(t *testing.T) {
	if !enableLocalSidecar(t) {
		t.Skip("langgraph venv not available")
	}
	resp := Run(context.Background(), RunRequest{
		ProjectID: "test-integration",
		Prompt:    "总结工作区内容",
	})
	if !resp.OK {
		t.Fatalf("expected ok=true, got error: %s", resp.Error)
	}
	r := resp.Result
	if !strings.Contains(r, "LangGraph") &&
		!strings.Contains(r, "工作区") &&
		!strings.Contains(r, "建议") {
		t.Errorf("result missing expected markers: %s", r[:min(len(r), 200)])
	}
	h := Health()
	if h["available"] != true {
		t.Fatalf("health should be available under test env: %v", h)
	}
}

func TestRunWithWorkspace(t *testing.T) {
	if !enableLocalSidecar(t) {
		t.Skip("langgraph venv not available")
	}
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "notes.md"), []byte("# hello\ncontent for graph\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "script.py"), []byte("print('ok')\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	resp := Run(ctx, RunRequest{
		ProjectID: "test-ws",
		Prompt:    "总结工作区",
		Workspace: dir,
	})
	if !resp.OK {
		t.Fatalf("expected ok=true, got error: %s", resp.Error)
	}
	if !strings.Contains(resp.Result, "notes.md") {
		t.Errorf("result should contain notes.md, got: %s", resp.Result[:min(len(resp.Result), 300)])
	}
	if !strings.Contains(resp.Result, "script.py") {
		t.Errorf("result should contain script.py, got: %s", resp.Result[:min(len(resp.Result), 300)])
	}
	if !strings.Contains(resp.Result, "建议") && !strings.Contains(resp.Result, "下一步") {
		t.Error("result should contain suggestions or next-steps")
	}
}

func TestRunNoWorkspace(t *testing.T) {
	if !enableLocalSidecar(t) {
		t.Skip("langgraph venv not available")
	}
	resp := Run(context.Background(), RunRequest{
		ProjectID: "no-ws",
		Prompt:    "分析",
	})
	if !resp.OK {
		t.Fatalf("expected ok=true even without workspace, got error: %s", resp.Error)
	}
	if !strings.Contains(resp.Result, "无") && !strings.Contains(resp.Result, "空") {
		t.Errorf("expected graceful no-workspace wording: %s", resp.Result[:min(len(resp.Result), 300)])
	}
}

func TestRunBadWorkspace(t *testing.T) {
	if !enableLocalSidecar(t) {
		t.Skip("langgraph venv not available")
	}
	resp := Run(context.Background(), RunRequest{
		ProjectID: "bad-ws",
		Prompt:    "分析",
		Workspace: "/nonexistent/path/xyz",
	})
	if !resp.OK {
		t.Fatalf("expected ok=true (runner handles bad path), got error: %s", resp.Error)
	}
	if !strings.Contains(resp.Result, "不存在") && !strings.Contains(resp.Result, "无") {
		t.Errorf("expected missing-path wording: %s", resp.Result[:min(len(resp.Result), 300)])
	}
}
