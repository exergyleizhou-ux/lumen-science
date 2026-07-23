package oasis

import (
	"os"
	"path/filepath"
	"testing"
)

func writeAlgoSrc(t *testing.T, dir, body string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, "algo.py"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

// VerifySource confirms the working tree is the exact source that produced the
// locked (and deployed) artifact: it recomputes the source hash and compares it
// to the lockfile's source_sha256.
func TestVerifySource_Match(t *testing.T) {
	dir := t.TempDir()
	writeAlgoSrc(t, dir, "print('hello')\n")
	h, err := ComputeSrcHash(dir)
	if err != nil {
		t.Fatal(err)
	}
	if err := WriteLock(dir, Lock{Manifest: Manifest{Name: "x"}, SrcHash: h, Digest: "sha256:d"}); err != nil {
		t.Fatal(err)
	}
	res, err := VerifySource(dir)
	if err != nil {
		t.Fatal(err)
	}
	if !res.SourceMatch {
		t.Errorf("expected match; current=%s locked=%s", res.CurrentHash, res.LockedHash)
	}
}

// If the source drifts after build/deploy, verify must FAIL (the locked digest no
// longer corresponds to this tree) and surface both hashes.
func TestVerifySource_Drift(t *testing.T) {
	dir := t.TempDir()
	writeAlgoSrc(t, dir, "print('hello')\n")
	h, _ := ComputeSrcHash(dir)
	if err := WriteLock(dir, Lock{Manifest: Manifest{Name: "x"}, SrcHash: h}); err != nil {
		t.Fatal(err)
	}
	writeAlgoSrc(t, dir, "print('TAMPERED')\n") // edit after locking
	res, err := VerifySource(dir)
	if err != nil {
		t.Fatal(err)
	}
	if res.SourceMatch {
		t.Error("expected drift to be detected, got match")
	}
	if res.CurrentHash == res.LockedHash || res.CurrentHash == "" {
		t.Errorf("drift result should surface a different current hash: cur=%s locked=%s", res.CurrentHash, res.LockedHash)
	}
}

// No lockfile → a clear error (nothing to verify against).
func TestVerifySource_NoLock(t *testing.T) {
	if _, err := VerifySource(t.TempDir()); err == nil {
		t.Error("expected an error when no lockfile is present")
	}
}
