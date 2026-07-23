package provenance

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestRecorderMCPImmediate(t *testing.T) {
	dir := t.TempDir()
	rec, err := NewRecorder(dir, "sess_test", "deepseek")
	if err != nil {
		t.Fatal(err)
	}
	rec.RecordMCP("pubmed", "search_articles", `{"query":"aspirin"}`)
	data, err := os.ReadFile(filepath.Join(dir, "provenance.jsonl"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), `"kind":"mcp_call"`) {
		t.Fatalf("missing mcp_call: %s", data)
	}
	if !strings.Contains(string(data), "pubmed/search_articles") {
		t.Fatalf("missing tool: %s", data)
	}
}

func TestRecorderAppend(t *testing.T) {
	dir := t.TempDir()
	ws := filepath.Join(dir, "workspace")
	_ = os.MkdirAll(filepath.Join(ws, "reports"), 0o700)
	path := filepath.Join(ws, "reports", "brief.md")
	_ = os.WriteFile(path, []byte("# brief"), 0o600)

	rec, err := NewRecorder(dir, "sess_test", "deepseek")
	if err != nil {
		t.Fatal(err)
	}
	if err := rec.RecordArtifact(path); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(filepath.Join(dir, "provenance.jsonl"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), "workspace/reports/brief.md") {
		t.Fatalf("missing path: %s", data)
	}
}
