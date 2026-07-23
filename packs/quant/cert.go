package quant

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
)

// CertFile is the re-computable backtest certificate emitted by `quant backtest`.
const CertFile = "quant-cert.json"

// Cert is a verifiable backtest certificate. Its ID is a hash of the provenance
// fields, so anyone re-running the pinned strategy on the pinned data in the
// pinned image gets the same metrics, equity-curve hash, and therefore the same
// VQ id. It attests "these numbers really came from this code on this data" — it
// makes no claim about future profitability.
type Cert struct {
	ID              string             `json:"cert_id"`
	EngineVersion   string             `json:"engine_version"`
	SrcHash         string             `json:"src_sha256"`
	DataHash        string             `json:"data_sha256"`
	ImageDigest     string             `json:"image_digest"`
	EquityCurveHash string             `json:"equity_curve_sha256"`
	Metrics         map[string]float64 `json:"metrics"`
	CreatedAt       string             `json:"created_at"`
	Recompute       string             `json:"recompute"`
}

// ComputeCertID hashes the provenance fields (everything that determines the
// result) into a stable VQ id. CreatedAt and the id itself are excluded, so the
// id depends only on what was actually run and what came out.
func ComputeCertID(c Cert) string {
	h := sha256.New()
	fmt.Fprintf(h, "engine|%s\n", c.EngineVersion)
	fmt.Fprintf(h, "src|%s\n", c.SrcHash)
	fmt.Fprintf(h, "data|%s\n", c.DataHash)
	fmt.Fprintf(h, "image|%s\n", c.ImageDigest)
	fmt.Fprintf(h, "equity|%s\n", c.EquityCurveHash)
	keys := make([]string, 0, len(c.Metrics))
	for k := range c.Metrics {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		fmt.Fprintf(h, "metric|%s|%.6f\n", k, c.Metrics[k])
	}
	return "VQ-" + hex.EncodeToString(h.Sum(nil))[:16]
}

// VerifyCert reports whether a cert's stored id matches a fresh recomputation of
// its provenance fields — i.e. no field was tampered after issuance.
func VerifyCert(c Cert) bool {
	return c.ID != "" && c.ID == ComputeCertID(c)
}

// WriteCert stamps the id and writes the cert to dir/quant-cert.json.
func WriteCert(dir string, c Cert) (Cert, error) {
	c.ID = ComputeCertID(c)
	b, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return Cert{}, err
	}
	return c, os.WriteFile(filepath.Join(dir, CertFile), b, 0o644)
}

// ReadCert reads dir/quant-cert.json.
func ReadCert(dir string) (Cert, error) {
	b, err := os.ReadFile(filepath.Join(dir, CertFile))
	if err != nil {
		return Cert{}, err
	}
	var c Cert
	if err := json.Unmarshal(b, &c); err != nil {
		return Cert{}, fmt.Errorf("parse %s: %w", CertFile, err)
	}
	return c, nil
}
