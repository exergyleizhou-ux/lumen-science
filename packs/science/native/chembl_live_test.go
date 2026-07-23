package native

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestChemblLiveOutput(t *testing.T) {
	if testing.Short() {
		t.Skip("live ChEMBL network test skipped in -short")
	}
	root, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	// repo root from internal/science/native
	for i := 0; i < 3; i++ {
		root = filepath.Dir(root)
	}
	bin := filepath.Join(root, "bin", "lumen-mcp-chembl")
	if _, err := os.Stat(bin); err != nil {
		t.Skip("bin not built")
	}
	t.Setenv("LUMEN_MCP_CHEMBL", bin)

	mgr := NewManager()
	defer mgr.Close()
	if err := mgr.Connect("chembl"); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 25*time.Second)
	defer cancel()
	text, err := mgr.CallToolCtx(ctx, "chembl", "search_compounds", map[string]any{"query": "aspirin", "limit": 1})
	if err != nil {
		t.Fatal(err)
	}
	t.Log(text)
	var probe struct {
		Provenance map[string]any `json:"provenance"`
		Results    any            `json:"results"`
	}
	if err := json.Unmarshal([]byte(text), &probe); err != nil {
		t.Fatalf("unmarshal: %v\nraw: %s", err, text)
	}
	if probe.Provenance == nil {
		t.Fatalf("missing provenance in: %s", text)
	}
}
