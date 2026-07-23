package quant

import (
	"crypto/ed25519"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"time"

	"lumen/internal/notary"
)

// AttestFile is the verifier's signed attestation of a backtest certificate.
const AttestFile = "quant-attestation.json"

// Attestation is a third-party verifier's signed statement that it independently
// re-ran a strategy and the backtest certificate reproduced. Buyers verify the
// Ed25519 signature against VerifierPubKey offline — a screenshot can be faked,
// this cannot. It attests reproducibility, not future profit.
type Attestation struct {
	CertID          string             `json:"cert_id"`
	SrcHash         string             `json:"src_sha256"`
	DataHash        string             `json:"data_sha256"`
	EquityCurveHash string             `json:"equity_curve_sha256"`
	Metrics         map[string]float64 `json:"metrics"`
	VerifierKeyID   string             `json:"verifier_key_id"`
	VerifierPubKey  string             `json:"verifier_pubkey"` // hex Ed25519 public key
	VerifiedAt      string             `json:"verified_at"`
	Signature       string             `json:"signature"` // hex Ed25519 sig over the canonical claims
}

// attestMessage is the canonical byte string that gets signed: every provenance
// claim, deterministically ordered, so tampering with any field breaks the sig.
func attestMessage(a Attestation) []byte {
	out := fmt.Sprintf("cert|%s\nsrc|%s\ndata|%s\nequity|%s\n",
		a.CertID, a.SrcHash, a.DataHash, a.EquityCurveHash)
	keys := make([]string, 0, len(a.Metrics))
	for k := range a.Metrics {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		out += fmt.Sprintf("metric|%s|%.6f\n", k, a.Metrics[k])
	}
	return []byte(out)
}

// SignAttestation builds and Ed25519-signs an attestation for a certificate.
func SignAttestation(cert Cert, kp *notary.KeyPair) Attestation {
	a := Attestation{
		CertID:          cert.ID,
		SrcHash:         cert.SrcHash,
		DataHash:        cert.DataHash,
		EquityCurveHash: cert.EquityCurveHash,
		Metrics:         cert.Metrics,
		VerifierKeyID:   kp.KeyID(),
		VerifierPubKey:  hex.EncodeToString(kp.PublicKey),
		VerifiedAt:      time.Now().UTC().Format(time.RFC3339),
	}
	a.Signature = hex.EncodeToString(kp.Sign(attestMessage(a)))
	return a
}

// VerifyAttestationSig checks the Ed25519 signature against the embedded public
// key — fully offline, no key registry needed.
func VerifyAttestationSig(a Attestation) bool {
	pub, err := hex.DecodeString(a.VerifierPubKey)
	if err != nil || len(pub) != ed25519.PublicKeySize {
		return false
	}
	sig, err := hex.DecodeString(a.Signature)
	if err != nil {
		return false
	}
	return ed25519.Verify(ed25519.PublicKey(pub), attestMessage(a), sig)
}

// AttestResult reports the outcome of checking an attestation in a directory.
type AttestResult struct {
	CertID         string
	SignatureValid bool // the verifier's signature checks out
	MatchesCert    bool // the attestation matches the cert present in the dir
	SourceMatch    bool // the working-tree source still hashes to the cert's src
	CertPresent    bool
}

// OK is true when the signature is valid and (if a cert is present) the
// attestation matches it AND the source has not drifted since it was signed.
func (r AttestResult) OK() bool {
	return r.SignatureValid && (!r.CertPresent || (r.MatchesCert && r.SourceMatch))
}

func WriteAttestation(dir string, a Attestation) error {
	b, err := json.MarshalIndent(a, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(dir, AttestFile), b, 0o644)
}

func ReadAttestation(dir string) (Attestation, error) {
	b, err := os.ReadFile(filepath.Join(dir, AttestFile))
	if err != nil {
		return Attestation{}, err
	}
	var a Attestation
	if err := json.Unmarshal(b, &a); err != nil {
		return Attestation{}, fmt.Errorf("parse %s: %w", AttestFile, err)
	}
	return a, nil
}

// CheckAttestation verifies the signature and cross-checks the attestation
// against the certificate in the directory (if one is present).
func CheckAttestation(dir string) (AttestResult, error) {
	a, err := ReadAttestation(dir)
	if err != nil {
		return AttestResult{}, err
	}
	res := AttestResult{CertID: a.CertID, SignatureValid: VerifyAttestationSig(a)}
	if cert, err := ReadCert(dir); err == nil {
		res.CertPresent = true
		res.MatchesCert = VerifyCert(cert) &&
			cert.ID == a.CertID &&
			cert.EquityCurveHash == a.EquityCurveHash &&
			cert.DataHash == a.DataHash
		// The signed cert pins a source hash; recompute it so a strategy edited
		// after attestation is caught even though the cert file is untouched.
		if cur, err := ComputeSrcHash(dir); err == nil {
			res.SourceMatch = cur == cert.SrcHash
		}
	}
	return res, nil
}

// Attest is the verifier role: independently re-run the backtest, confirm it
// reproduces the certificate bit-for-bit, then sign. A non-reproducing or
// drifted strategy is refused — the signature only ever covers a real result.
func Attest(dir string, kp *notary.KeyPair, opts BacktestOptions) (Attestation, error) {
	v, err := VerifyBacktest(dir, opts)
	if err != nil {
		return Attestation{}, err
	}
	if !v.OK() {
		return Attestation{}, fmt.Errorf("refusing to attest: backtest did not reproduce (source match=%v, reproduces=%v)", v.SourceMatch, v.Reproduces)
	}
	cert, err := ReadCert(dir)
	if err != nil {
		return Attestation{}, err
	}
	att := SignAttestation(cert, kp)
	if err := WriteAttestation(dir, att); err != nil {
		return Attestation{}, err
	}
	return att, nil
}

// LoadOrCreateVerifierKey reads an Ed25519 keypair from path (hex JSON), creating
// and saving one if absent. This is the verifier's identity.
func LoadOrCreateVerifierKey(path string) (*notary.KeyPair, error) {
	if b, err := os.ReadFile(path); err == nil {
		var rec struct {
			Label      string `json:"label"`
			PublicKey  string `json:"public_key"`
			PrivateKey string `json:"private_key"`
		}
		if err := json.Unmarshal(b, &rec); err != nil {
			return nil, err
		}
		priv, err := hex.DecodeString(rec.PrivateKey)
		if err != nil {
			return nil, err
		}
		pub, err := hex.DecodeString(rec.PublicKey)
		if err != nil {
			return nil, err
		}
		return &notary.KeyPair{Label: rec.Label, PublicKey: ed25519.PublicKey(pub), PrivateKey: ed25519.PrivateKey(priv)}, nil
	}
	kp, err := notary.GenerateKeyPair("quant-verifier")
	if err != nil {
		return nil, err
	}
	rec := map[string]string{
		"label":       kp.Label,
		"public_key":  hex.EncodeToString(kp.PublicKey),
		"private_key": hex.EncodeToString(kp.PrivateKey),
	}
	b, _ := json.MarshalIndent(rec, "", "  ")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	if err := os.WriteFile(path, b, 0o600); err != nil {
		return nil, err
	}
	return kp, nil
}
