package quant

import (
	"os"
	"path/filepath"
	"testing"
)

func writeFile(t *testing.T, dir, name, body string) {
	t.Helper()
	p := filepath.Join(dir, name)
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestComputeSrcHashExcludesGeneratedArtifacts(t *testing.T) {
	dir := t.TempDir()
	writeFile(t, dir, "strategy.py", "class Strategy:\n    pass\n")
	writeFile(t, dir, "quant.toml", "name = \"x\"\n")
	before, err := ComputeSrcHash(dir)
	if err != nil {
		t.Fatal(err)
	}

	// Generated outputs and caches must not change the source hash.
	writeFile(t, dir, CertFile, `{"cert_id":"VQ-1"}`)
	writeFile(t, dir, LockFile, `{}`)
	writeFile(t, dir, "results.json", `{}`)
	writeFile(t, dir, "__pycache__/strategy.cpython-39.pyc", "junk")
	after, err := ComputeSrcHash(dir)
	if err != nil {
		t.Fatal(err)
	}
	if before != after {
		t.Fatalf("artifacts changed src hash: %s -> %s", before, after)
	}
}

func TestComputeSrcHashChangesWithSource(t *testing.T) {
	dir := t.TempDir()
	writeFile(t, dir, "strategy.py", "class Strategy:\n    pass\n")
	a, _ := ComputeSrcHash(dir)
	writeFile(t, dir, "strategy.py", "class Strategy:\n    x = 1\n")
	b, _ := ComputeSrcHash(dir)
	if a == b {
		t.Fatal("editing strategy.py must change the src hash")
	}
}
