package oasis

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func sampleLock() Lock {
	return Lock{
		Manifest: Manifest{Name: "logreg", Runtime: "docker", Image: "reg/algo/logreg", OutputKind: "model", Version: 2},
		BuiltAt:  "2026-06-20T00:00:00Z",
		Image:    "reg/algo/logreg",
		Digest:   "sha256:abc123",
		SrcHash:  "deadbeefcafebabe",
	}
}

// MarshalLock must serialize the FULL provenance record — the old hand-rolled
// encoder silently dropped manifest + image_digest, gutting the lockfile.
func TestMarshalLock_IsComplete(t *testing.T) {
	b, err := MarshalLock(sampleLock())
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{`"name": "logreg"`, `"image_digest": "sha256:abc123"`, `"source_sha256": "deadbeefcafebabe"`, `"built_at"`} {
		if !strings.Contains(string(b), want) {
			t.Errorf("lockfile JSON missing %q\n--- got ---\n%s", want, b)
		}
	}
}

func TestWriteReadLock_RoundTrip(t *testing.T) {
	dir := t.TempDir()
	want := sampleLock()
	if err := WriteLock(dir, want); err != nil {
		t.Fatal(err)
	}
	got, err := ReadLock(dir)
	if err != nil {
		t.Fatal(err)
	}
	if got.Manifest.Name != want.Manifest.Name || got.Digest != want.Digest || got.SrcHash != want.SrcHash || got.Manifest.Version != want.Manifest.Version {
		t.Errorf("round-trip lost fields: got %+v want %+v", got, want)
	}
	if _, err := os.Stat(filepath.Join(dir, "oasis-lock.json")); err != nil {
		t.Errorf("lockfile not written at oasis-lock.json: %v", err)
	}
}

// After deploy resolves the registry digest, it must be pinned back into the
// lockfile WITHOUT losing the rest of the provenance record.
func TestUpdateLockDigest_PinsWithoutLoss(t *testing.T) {
	dir := t.TempDir()
	l := sampleLock()
	l.Digest = "" // build-time: not yet known
	if err := WriteLock(dir, l); err != nil {
		t.Fatal(err)
	}
	if err := UpdateLockDigest(dir, "sha256:final99"); err != nil {
		t.Fatal(err)
	}
	got, err := ReadLock(dir)
	if err != nil {
		t.Fatal(err)
	}
	if got.Digest != "sha256:final99" {
		t.Errorf("digest = %q, want sha256:final99", got.Digest)
	}
	if got.Manifest.Name != "logreg" || got.SrcHash != "deadbeefcafebabe" {
		t.Errorf("digest write-back clobbered provenance: %+v", got)
	}
}
