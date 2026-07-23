package quant

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// LockFile is the provenance record written next to a backtested strategy. It
// pins the manifest, source hash, dataset hash, image digest, the result
// fingerprint, and the cert id — the re-verifiable record behind a VQ cert.
const LockFile = "quant-lock.json"

// Lock is the quant provenance record (mirrors oasis.Lock, extended for the
// backtest's data + result hashes).
type Lock struct {
	Manifest        Manifest `json:"manifest"`
	BuiltAt         string   `json:"built_at"`
	Image           string   `json:"image"`
	Digest          string   `json:"image_digest"`
	SrcHash         string   `json:"source_sha256"`
	DataHash        string   `json:"data_sha256"`
	EquityCurveHash string   `json:"equity_curve_sha256"`
	CertID          string   `json:"cert_id"`
}

// WriteLock writes the lockfile into dir.
func WriteLock(dir string, l Lock) error {
	b, err := json.MarshalIndent(l, "", "  ")
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
