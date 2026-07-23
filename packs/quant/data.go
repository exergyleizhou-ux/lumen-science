package quant

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"
)

// DataLockFile records the provenance of a fetched dataset.
const DataLockFile = "data.lock"

// DataLock pins where the data.csv came from, so a backtest's data hash can be
// traced back to a concrete fetch (source, universe, window).
type DataLock struct {
	Source     string `json:"source"`
	Universe   string `json:"universe"`
	Start      string `json:"start"`
	End        string `json:"end"`
	FetchedAt  string `json:"fetched_at"`
	FileSHA256 string `json:"file_sha256"`
}

// RunDataFetch fetches the manifest's universe over its window and writes
// dir/data.csv plus dir/data.lock. Only source "akshare" performs a fetch;
// "csv" means the user supplies data.csv themselves (no-op with a clear error).
func RunDataFetch(dir, python string) (DataLock, error) {
	m, err := LoadManifest(dir)
	if err != nil {
		return DataLock{}, err
	}
	if m.DataSource != "akshare" {
		return DataLock{}, fmt.Errorf("data_source = %q: nothing to fetch (set data_source=\"akshare\" in %s, or supply data.csv yourself)", m.DataSource, ManifestFile)
	}
	if python == "" {
		python = "python3"
	}

	work, err := os.MkdirTemp("", "quant-fetch-")
	if err != nil {
		return DataLock{}, err
	}
	defer os.RemoveAll(work)
	fetchPy := filepath.Join(work, "fetch.py")
	if err := os.WriteFile(fetchPy, fetchScript, 0o644); err != nil {
		return DataLock{}, err
	}

	out := filepath.Join(dir, "data.csv")
	cmd := exec.Command(python, fetchPy,
		"--universe", m.Universe, "--start", m.Start, "--end", m.End, "--out", out)
	cmd.Stderr = os.Stderr
	if o, err := cmd.Output(); err != nil {
		return DataLock{}, fmt.Errorf("fetch failed: %w", err)
	} else {
		os.Stdout.Write(o)
	}

	sum, err := fileSHA256(out)
	if err != nil {
		return DataLock{}, err
	}
	lock := DataLock{
		Source: m.DataSource, Universe: m.Universe, Start: m.Start, End: m.End,
		FetchedAt: time.Now().UTC().Format(time.RFC3339), FileSHA256: sum,
	}
	b, _ := json.MarshalIndent(lock, "", "  ")
	if err := os.WriteFile(filepath.Join(dir, DataLockFile), b, 0o644); err != nil {
		return DataLock{}, err
	}
	return lock, nil
}

func fileSHA256(path string) (string, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	s := sha256.Sum256(b)
	return hex.EncodeToString(s[:]), nil
}
