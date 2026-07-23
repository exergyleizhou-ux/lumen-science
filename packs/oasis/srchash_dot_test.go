package oasis

import (
	"os"
	"path/filepath"
	"testing"
)

// ComputeSrcHash(".") is the common case (`lumen oasis build` / `build .`). The
// walk root's own Name() is "." — the dot-prefix hidden-dir skip must NOT fire on
// the root, or it SkipDir's the entire tree and hashes nothing (the empty-input
// digest), silently gutting the provenance record.
func TestComputeSrcHash_RelativeDotRootHashesFiles(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "train.py"), []byte("print('hi')\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "Dockerfile"), []byte("FROM python:3.11-slim\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// The empty-input SHA-256 — what a tree that hashed nothing would produce.
	const emptyHash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

	abs, err := ComputeSrcHash(dir)
	if err != nil {
		t.Fatal(err)
	}
	if abs == emptyHash {
		t.Fatal("absolute-dir hash is the empty hash — fixture wrote no files?")
	}

	// Now hash the SAME tree addressed as "." (chdir in).
	wd, _ := os.Getwd()
	defer os.Chdir(wd)
	if err := os.Chdir(dir); err != nil {
		t.Fatal(err)
	}
	dot, err := ComputeSrcHash(".")
	if err != nil {
		t.Fatal(err)
	}
	if dot == emptyHash {
		t.Fatal(`ComputeSrcHash(".") hashed nothing — the root "." was skipped as a hidden dir`)
	}
	if dot != abs {
		t.Errorf(`ComputeSrcHash(".") = %s, want it to equal the absolute-path hash %s`, dot, abs)
	}
}
