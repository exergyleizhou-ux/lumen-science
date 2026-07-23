package runtime

import (
	"os"
	"path/filepath"
	"testing"
)

func TestResolvePythonOperonMCP(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Skip(err)
	}
	dataDir := filepath.Join(home, ".lumen", "science", "sandbox", "home", ".claude-science")
	if _, err := os.Stat(filepath.Join(dataDir, "conda", "envs", "operon-mcp")); err != nil {
		t.Skip("no operon-mcp env in research pack")
	}
	py := ResolvePython(dataDir)
	if !isExecutable(py) {
		t.Fatalf("ResolvePython returned non-executable %q", py)
	}
	if filepath.Base(py) == "python3" && py == "python3" {
		t.Fatal("expected pack python, got bare python3")
	}
}

func TestResolvePythonMissingPack(t *testing.T) {
	if got := ResolvePython(""); got != "python3" {
		t.Fatalf("got %q", got)
	}
}
