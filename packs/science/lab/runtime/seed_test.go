package runtime

import (
	"os"
	"path/filepath"
	"testing"
)

func TestSeedExampleDirLive(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Skip(err)
	}
	sci := filepath.Join(home, ".lumen", "science")
	src := SeedExampleDir(sci, "example_crispr_screen")
	if src == "" {
		t.Skip("research pack seed not present")
	}
	ws := filepath.Join(t.TempDir(), "workspace")
	if err := CopySeedExample(sci, "example_crispr_screen", ws); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(ws, "data", "example_crispr_screen", "README.md")); err != nil {
		t.Fatalf("seed copy missing README: %v", err)
	}
}
