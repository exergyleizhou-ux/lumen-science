package quant

import "testing"

func sampleCert() Cert {
	return Cert{
		EngineVersion:   "quant-engine/0.1.0",
		SrcHash:         "aaaa",
		DataHash:        "bbbb",
		ImageDigest:     "sha256:cccc",
		EquityCurveHash: "dddd",
		Metrics: map[string]float64{
			"total_return": 0.25, "sharpe": 1.4, "max_drawdown": -0.1,
		},
		CreatedAt: "2026-06-27T00:00:00Z",
	}
}

func TestCertIDIsDeterministic(t *testing.T) {
	a := ComputeCertID(sampleCert())
	b := ComputeCertID(sampleCert())
	if a != b {
		t.Fatalf("cert id not deterministic: %q != %q", a, b)
	}
	if len(a) < 4 || a[:3] != "VQ-" {
		t.Fatalf("cert id should be VQ-prefixed, got %q", a)
	}
}

func TestCertIDIgnoresCreatedAt(t *testing.T) {
	c1 := sampleCert()
	c2 := sampleCert()
	c2.CreatedAt = "1999-01-01T00:00:00Z"
	if ComputeCertID(c1) != ComputeCertID(c2) {
		t.Fatal("created_at must not affect the cert id (only provenance fields do)")
	}
}

func TestCertIDChangesWithEveryProvenanceField(t *testing.T) {
	base := ComputeCertID(sampleCert())
	cases := map[string]func(*Cert){
		"src":    func(c *Cert) { c.SrcHash = "zzzz" },
		"data":   func(c *Cert) { c.DataHash = "zzzz" },
		"image":  func(c *Cert) { c.ImageDigest = "sha256:zzzz" },
		"equity": func(c *Cert) { c.EquityCurveHash = "zzzz" },
		"metric": func(c *Cert) { c.Metrics["sharpe"] = 9.9 },
		"engine": func(c *Cert) { c.EngineVersion = "x/9" },
	}
	for name, mutate := range cases {
		c := sampleCert()
		mutate(&c)
		if ComputeCertID(c) == base {
			t.Errorf("cert id did not change when %s changed", name)
		}
	}
}

func TestVerifyCertDetectsTamper(t *testing.T) {
	c := sampleCert()
	c.ID = ComputeCertID(c)
	if !VerifyCert(c) {
		t.Fatal("freshly issued cert should verify")
	}
	c.Metrics["total_return"] = 9.99 // fake a better return
	if VerifyCert(c) {
		t.Fatal("tampered metrics must fail verification")
	}
}
