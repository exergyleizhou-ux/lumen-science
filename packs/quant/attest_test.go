package quant

import (
	"os"
	"path/filepath"
	"testing"

	"lumen/internal/notary"
)

func issuedCert() Cert {
	c := Cert{
		EngineVersion:   "quant-engine/0.1.0",
		SrcHash:         "src-aaaa",
		DataHash:        "data-bbbb",
		ImageDigest:     "sha256:cccc",
		EquityCurveHash: "eq-dddd",
		Metrics:         map[string]float64{"total_return": 0.25, "sharpe": 1.4},
		CreatedAt:       "2026-06-28T00:00:00Z",
	}
	c.ID = ComputeCertID(c)
	return c
}

func TestSignedAttestationVerifies(t *testing.T) {
	kp, err := notary.GenerateKeyPair("verifier")
	if err != nil {
		t.Fatal(err)
	}
	att := SignAttestation(issuedCert(), kp)
	if att.VerifierKeyID != kp.KeyID() {
		t.Errorf("key id = %q, want %q", att.VerifierKeyID, kp.KeyID())
	}
	if !VerifyAttestationSig(att) {
		t.Fatal("freshly signed attestation should verify")
	}
}

func TestTamperedClaimBreaksSignature(t *testing.T) {
	kp, _ := notary.GenerateKeyPair("verifier")
	att := SignAttestation(issuedCert(), kp)

	bad := att
	bad.Metrics = map[string]float64{"total_return": 9.99, "sharpe": 1.4} // faked return
	if VerifyAttestationSig(bad) {
		t.Error("tampering the metrics must invalidate the signature")
	}

	bad2 := att
	bad2.Signature = att.Signature[:len(att.Signature)-2] + "00"
	if VerifyAttestationSig(bad2) {
		t.Error("a mangled signature must not verify")
	}
}

func TestCheckAttestationCrossChecksLocalCert(t *testing.T) {
	dir := t.TempDir()
	cert := issuedCert()
	if _, err := WriteCert(dir, cert); err != nil {
		t.Fatal(err)
	}
	kp, _ := notary.GenerateKeyPair("verifier")
	att := SignAttestation(cert, kp)
	if err := WriteAttestation(dir, att); err != nil {
		t.Fatal(err)
	}

	res, err := CheckAttestation(dir)
	if err != nil {
		t.Fatal(err)
	}
	// This fixture has no source tree; OK()'s source-drift check is covered by
	// TestSourceDriftFailsAttestation. Here we assert the signature + cert match.
	if !res.SignatureValid || !res.MatchesCert {
		t.Fatalf("clean attestation should pass sig + cert checks: %+v", res)
	}

	// Tamper the local cert's metrics — the attestation must no longer match.
	cert.Metrics["total_return"] = 9.99
	cert.ID = ComputeCertID(cert)
	b, _ := os.ReadFile(filepath.Join(dir, CertFile))
	_ = b
	if _, err := WriteCert(dir, cert); err != nil {
		t.Fatal(err)
	}
	res2, _ := CheckAttestation(dir)
	if res2.MatchesCert {
		t.Error("a cert tampered after attestation must fail the cross-check")
	}
}

func TestSourceDriftFailsAttestation(t *testing.T) {
	dir := t.TempDir()
	writeFile(t, dir, "strategy.py", "class Strategy:\n    pass\n")
	src, _ := ComputeSrcHash(dir)
	cert := issuedCert()
	cert.SrcHash = src
	cert.ID = ComputeCertID(cert)
	if _, err := WriteCert(dir, cert); err != nil {
		t.Fatal(err)
	}
	kp, _ := notary.GenerateKeyPair("verifier")
	if err := WriteAttestation(dir, SignAttestation(cert, kp)); err != nil {
		t.Fatal(err)
	}

	clean, _ := CheckAttestation(dir)
	if !clean.SourceMatch || !clean.OK() {
		t.Fatalf("unchanged source should pass: %+v", clean)
	}

	// Edit the strategy after attestation — the signed cert no longer matches code.
	writeFile(t, dir, "strategy.py", "class Strategy:\n    x = 1\n")
	drift, _ := CheckAttestation(dir)
	if drift.SourceMatch {
		t.Error("source drift must be detected")
	}
	if drift.OK() {
		t.Error("attestation must be INVALID once the source drifts from the cert")
	}
}
