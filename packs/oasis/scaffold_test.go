package oasis

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// The scaffold emits a pure-Python-stdlib algorithm — matching every real
// marketplace C2D algorithm (kmeans/logreg/gnb): tiny audit surface, no build
// toolchain, and the REAL container contract (/data + /params.json -> /out/output.bin).

func TestScaffoldedAlgoIsValidPython(t *testing.T) {
	py, err := exec.LookPath("python3")
	if err != nil {
		t.Skip("python3 not in PATH")
	}
	dir := t.TempDir()
	if err := Scaffold(dir, DefaultManifest("demo")); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command(py, "-m", "py_compile", filepath.Join(dir, "train.py"))
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("scaffolded train.py should be valid python out of the box: %v\n%s", err, out)
	}
}

func TestScaffoldFollowsRealContract(t *testing.T) {
	dir := t.TempDir()
	if err := Scaffold(dir, DefaultManifest("demo")); err != nil {
		t.Fatal(err)
	}
	src, err := os.ReadFile(filepath.Join(dir, "train.py"))
	if err != nil {
		t.Fatal(err)
	}
	s := string(src)
	for _, want := range []string{"/data", "/params.json", "/out", "output.bin", "model.json", "metrics.json"} {
		if !strings.Contains(s, want) {
			t.Errorf("train.py should reference %q (the real C2D contract)", want)
		}
	}
	// The old Go template's bug: it read params from /out/input.json, which the
	// runner does NOT mount — params are at /params.json.
	if strings.Contains(s, "/out/input.json") {
		t.Error("train.py must not use /out/input.json — the runner mounts params at /params.json")
	}
	// No Go scaffolding should remain.
	for _, gone := range []string{filepath.Join("cmd", "algo", "main.go"), "go.mod"} {
		if _, err := os.Stat(filepath.Join(dir, gone)); err == nil {
			t.Errorf("scaffold should no longer emit %s", gone)
		}
	}
}

func TestScaffoldDockerfileIsPythonNonRoot(t *testing.T) {
	dir := t.TempDir()
	if err := Scaffold(dir, DefaultManifest("demo")); err != nil {
		t.Fatal(err)
	}
	df, err := os.ReadFile(filepath.Join(dir, "Dockerfile"))
	if err != nil {
		t.Fatal(err)
	}
	s := string(df)
	if !strings.Contains(s, "python") {
		t.Errorf("Dockerfile should be python-based, got:\n%s", s)
	}
	if !strings.Contains(s, "65534") {
		t.Errorf("Dockerfile should run as the non-root nobody uid 65534, got:\n%s", s)
	}
}

func TestDefaultManifestImageUntagged(t *testing.T) {
	m := DefaultManifest("demo")
	// Untagged so `oasis build` (via ImageTag) can append the version cleanly,
	// instead of the old broken ":latest:1".
	if got := ImageTag(m.Image, m.Version); got != m.Image+":1" {
		t.Errorf("default image %q should be untagged so ImageTag appends :1, got %q", m.Image, got)
	}
}

func TestComputeSrcHashCoversNonGoFiles(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "train.py"), []byte("print('a')\n"), 0644); err != nil {
		t.Fatal(err)
	}
	h1, err := ComputeSrcHash(dir)
	if err != nil {
		t.Fatal(err)
	}
	const emptyTree = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" // sha256 of nothing
	if h1 == "" || h1 == emptyTree {
		t.Fatalf("ComputeSrcHash ignored the python source (got empty-tree hash %q) — provenance must cover non-.go files", h1)
	}
	if err := os.WriteFile(filepath.Join(dir, "train.py"), []byte("print('b')\n"), 0644); err != nil {
		t.Fatal(err)
	}
	h2, _ := ComputeSrcHash(dir)
	if h2 == h1 {
		t.Error("ComputeSrcHash should change when the source changes")
	}
}

func TestComputeSrcHashNoCollision(t *testing.T) {
	// Distinct trees must not alias to the same digest. Without per-record
	// framing, {ab:"X", c:"Y"} and {a:"bXcY"} concatenate to the same bytes.
	d1 := t.TempDir()
	if err := os.WriteFile(filepath.Join(d1, "ab"), []byte("X"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(d1, "c"), []byte("Y"), 0644); err != nil {
		t.Fatal(err)
	}
	d2 := t.TempDir()
	if err := os.WriteFile(filepath.Join(d2, "a"), []byte("bXcY"), 0644); err != nil {
		t.Fatal(err)
	}
	h1, _ := ComputeSrcHash(d1)
	h2, _ := ComputeSrcHash(d2)
	if h1 == h2 {
		t.Errorf("distinct source trees collided to the same hash: %s", h1)
	}
}
