package oasis

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// LockFile is the provenance record written next to a built algorithm. It pins
// the exact manifest, source hash, and (after deploy) the registry image digest
// — the re-verifiable record that backs the marketplace's signed result certs.
const LockFile = "oasis-lock.json"

// MarshalLock serializes a Lock as the complete, indented provenance record.
// (Replaces a hand-rolled encoder that silently dropped manifest + image_digest.)
func MarshalLock(l Lock) ([]byte, error) {
	return json.MarshalIndent(l, "", "  ")
}

// WriteLock writes the lockfile into dir.
func WriteLock(dir string, l Lock) error {
	b, err := MarshalLock(l)
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(dir, LockFile), b, 0o644)
}

// ReadLock reads and parses the lockfile from dir.
func ReadLock(dir string) (Lock, error) {
	b, err := os.ReadFile(filepath.Join(dir, LockFile))
	if err != nil {
		return Lock{}, err
	}
	var l Lock
	if err := json.Unmarshal(b, &l); err != nil {
		return Lock{}, fmt.Errorf("parse %s: %w", LockFile, err)
	}
	return l, nil
}

// UpdateLockDigest pins the resolved registry image digest into the existing
// lockfile, preserving the rest of the provenance record. Called after `deploy`
// resolves the digest so the author ends with a complete, re-verifiable record.
func UpdateLockDigest(dir, digest string) error {
	l, err := ReadLock(dir)
	if err != nil {
		return err
	}
	l.Digest = digest
	return WriteLock(dir, l)
}
