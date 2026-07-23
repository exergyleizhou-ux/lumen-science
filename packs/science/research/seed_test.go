package research

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestEnsureOrgPack(t *testing.T) {
	dir := t.TempDir()
	org := "8f6f5807-90a0-4555-8c56-6eab2965e4ad"
	if err := EnsureOrgPack(dir, org); err != nil {
		t.Fatal(err)
	}
	prefsPath := filepath.Join(dir, "orgs", org, "preferences.json")
	data, err := os.ReadFile(prefsPath)
	if err != nil {
		t.Fatal(err)
	}
	var prefs map[string]any
	if err := json.Unmarshal(data, &prefs); err != nil {
		t.Fatal(err)
	}
	skip, _ := prefs["skipMcpApprovals"].(map[string]any)
	if skip["bundled:genomes"] != true {
		t.Fatalf("expected bundled:genomes auto-approve")
	}
	if skip["bundled:ketcher-chemistry"] != true {
		t.Fatalf("expected ketcher auto-approve")
	}
	ws := filepath.Join(dir, "orgs", org, "workspaces", "_mcp-ketcher-chemistry", ".cache")
	if _, err := os.Stat(ws); err != nil {
		t.Fatal(err)
	}
	if err := EnsureOrgPack(dir, org); err != nil {
		t.Fatal(err)
	}
}
