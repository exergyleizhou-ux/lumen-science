package oasis

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestDefaultManifest(t *testing.T) {
	m := DefaultManifest("test-algo")
	if m.Name != "test-algo" {
		t.Errorf("Name=%q", m.Name)
	}
	if m.Runtime != "docker" {
		t.Errorf("Runtime=%q", m.Runtime)
	}
	if m.OutputKind != "model" {
		t.Errorf("OutputKind=%q", m.OutputKind)
	}
	if m.Version != 1 {
		t.Errorf("Version=%d", m.Version)
	}
}

func TestValidate_ok(t *testing.T) {
	m := DefaultManifest("my-algo")
	m.Image = "registry.example.com/my-algo:latest"
	m.Digest = "sha256:abc123"
	errs := Validate(m)
	if len(errs) > 0 {
		t.Errorf("unexpected errors: %v", errs)
	}
}

func TestValidate_missingName(t *testing.T) {
	m := DefaultManifest("")
	m.Name = ""
	m.Image = "test:latest"
	errs := Validate(m)
	if len(errs) == 0 {
		t.Fatal("expected error for missing name")
	}
	found := false
	for _, e := range errs {
		if strings.Contains(e, "name") {
			found = true
		}
	}
	if !found {
		t.Errorf("expected 'name is required' error, got: %v", errs)
	}
}

func TestValidate_missingImage(t *testing.T) {
	m := DefaultManifest("test")
	m.Image = ""
	errs := Validate(m)
	found := false
	for _, e := range errs {
		if strings.Contains(e, "image") {
			found = true
		}
	}
	if !found {
		t.Errorf("expected 'image is required' error, got: %v", errs)
	}
}

func TestValidate_invalidRuntime(t *testing.T) {
	m := DefaultManifest("test")
	m.Runtime = "kubernetes"
	m.Image = "test:latest"
	errs := Validate(m)
	found := false
	for _, e := range errs {
		if strings.Contains(e, "runtime") {
			found = true
		}
	}
	if !found {
		t.Errorf("expected runtime error, got: %v", errs)
	}
}

func TestValidate_invalidOutputKind(t *testing.T) {
	m := DefaultManifest("test")
	m.OutputKind = "video"
	m.Image = "test:latest"
	errs := Validate(m)
	found := false
	for _, e := range errs {
		if strings.Contains(e, "output_kind") {
			found = true
		}
	}
	if !found {
		t.Errorf("expected output_kind error, got: %v", errs)
	}
}

func TestScaffold(t *testing.T) {
	dir := t.TempDir()
	algoDir := filepath.Join(dir, "my-algo")
	m := DefaultManifest("my-algo")
	if err := Scaffold(algoDir, m); err != nil {
		t.Fatalf("scaffold: %v", err)
	}

	// Check files exist
	for _, name := range []string{"oasis.toml", "Dockerfile", ".gitignore"} {
		if _, err := os.Stat(filepath.Join(algoDir, name)); err != nil {
			t.Errorf("missing file %s: %v", name, err)
		}
	}
	if _, err := os.Stat(filepath.Join(algoDir, "train.py")); err != nil {
		t.Errorf("missing train.py: %v", err)
	}

	// Check manifest content
	data, err := os.ReadFile(filepath.Join(algoDir, "oasis.toml"))
	if err != nil {
		t.Fatalf("read manifest: %v", err)
	}
	if !strings.Contains(string(data), "my-algo") {
		t.Errorf("manifest missing algo name")
	}
}

func TestParseManifest(t *testing.T) {
	raw := `name = "my-algo"
runtime = "docker"
image = "registry.example.com/my-algo:latest"
output_kind = "model"
version = 3`
	m, err := ParseManifest(raw)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if m.Name != "my-algo" {
		t.Errorf("Name=%q", m.Name)
	}
	if m.Version != 3 {
		t.Errorf("Version=%d", m.Version)
	}
	if m.OutputKind != "model" {
		t.Errorf("OutputKind=%q", m.OutputKind)
	}
}

func TestComputeSrcHash(t *testing.T) {
	dir := t.TempDir()
	// Create a known Go file
	os.WriteFile(filepath.Join(dir, "main.go"), []byte("package main\nfunc main() {}\n"), 0644)
	hash1, err := ComputeSrcHash(dir)
	if err != nil {
		t.Fatalf("hash: %v", err)
	}
	if hash1 == "" {
		t.Fatal("empty hash")
	}

	// Same content should give same hash
	hash2, err := ComputeSrcHash(dir)
	if err != nil {
		t.Fatalf("hash2: %v", err)
	}
	if hash1 != hash2 {
		t.Errorf("hash mismatch: %s vs %s", hash1, hash2)
	}
}

func TestParamsSchemaRoundTrip(t *testing.T) {
	// A real params_schema must survive formatTOML -> ParseManifest intact (the
	// %q-escaped JSON was previously trimmed but never unescaped, silently
	// corrupting it and getting dropped from the register payload).
	m := DefaultManifest("demo")
	m.ParamsSchema = `{"n_estimators":{"type":"integer","default":100}}`
	parsed, err := ParseManifest(formatTOML(m))
	if err != nil {
		t.Fatal(err)
	}
	if parsed.ParamsSchema != m.ParamsSchema {
		t.Errorf("params_schema round-trip lost data:\n  in:  %s\n  out: %s", m.ParamsSchema, parsed.ParamsSchema)
	}
	if !json.Valid([]byte(parsed.ParamsSchema)) {
		t.Errorf("round-tripped params_schema is not valid JSON: %s", parsed.ParamsSchema)
	}
}
