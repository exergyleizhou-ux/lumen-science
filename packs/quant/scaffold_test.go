package quant

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestScaffoldWritesARunnablePackage(t *testing.T) {
	dir := t.TempDir()
	if err := ScaffoldStrategy(dir, DefaultManifest("mom")); err != nil {
		t.Fatalf("ScaffoldStrategy: %v", err)
	}

	for _, f := range []string{ManifestFile, "strategy.py", "data.csv", ".gitignore"} {
		if _, err := os.Stat(filepath.Join(dir, f)); err != nil {
			t.Errorf("missing scaffolded file %s: %v", f, err)
		}
	}

	raw, _ := os.ReadFile(filepath.Join(dir, ManifestFile))
	m, err := ParseManifest(string(raw))
	if err != nil {
		t.Fatalf("scaffolded manifest does not parse: %v", err)
	}
	if m.Name != "mom" {
		t.Errorf("manifest name = %q, want mom", m.Name)
	}

	strat, _ := os.ReadFile(filepath.Join(dir, "strategy.py"))
	if !strings.Contains(string(strat), "class Strategy") {
		t.Error("strategy.py should define a Strategy class")
	}
	if !strings.Contains(string(strat), "def on_bar") {
		t.Error("strategy.py should define on_bar")
	}
}

func TestHarnessFilesAreEmbedded(t *testing.T) {
	// The engine the sandbox runs must travel with the binary.
	for _, name := range []string{"engine.py", "data.py", "rules.py", "metrics.py", "dataset.py", "run.py"} {
		if _, ok := HarnessFiles[name]; !ok {
			t.Errorf("harness file %s not embedded", name)
		}
	}
}
