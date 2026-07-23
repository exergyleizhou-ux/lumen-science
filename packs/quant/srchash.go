package quant

import (
	"crypto/sha256"
	"encoding/hex"
	"io"
	"os"
	"path/filepath"
	"strings"
)

// ComputeSrcHash hashes the strategy package's source tree (strategy.py,
// quant.toml, any helper modules) so a cert can pin the exact code that ran.
//
// Mirrors oasis.ComputeSrcHash, including its fixes: never SkipDir the walk root
// (so `quant backtest .` doesn't hash nothing), and frame each file as
// sha256(rel) || sha256(content) so distinct trees can't alias. Generated
// artifacts (lockfile, cert, results) and caches are excluded so re-hashing an
// unchanged strategy is stable.
func ComputeSrcHash(dir string) (string, error) {
	h := sha256.New()
	err := filepath.Walk(dir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		name := info.Name()
		if info.IsDir() {
			if path == dir {
				return nil // never skip the root
			}
			if name == ".git" || name == "vendor" || name == "__pycache__" || strings.HasPrefix(name, ".") {
				return filepath.SkipDir
			}
			return nil
		}
		if isGeneratedArtifact(name) {
			return nil
		}
		rel, err := filepath.Rel(dir, path)
		if err != nil {
			return err
		}
		f, err := os.Open(path)
		if err != nil {
			return err
		}
		fh := sha256.New()
		_, cerr := io.Copy(fh, f)
		f.Close()
		if cerr != nil {
			return cerr
		}
		relSum := sha256.Sum256([]byte(rel))
		h.Write(relSum[:])
		h.Write(fh.Sum(nil))
		return nil
	})
	if err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

func isGeneratedArtifact(name string) bool {
	switch name {
	case LockFile, CertFile, AttestFile, DataLockFile, "results.json":
		return true
	}
	return strings.HasSuffix(name, ".pyc")
}
