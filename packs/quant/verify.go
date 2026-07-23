package quant

import "fmt"

// VerifyResult reports whether a strategy still reproduces its certificate.
type VerifyResult struct {
	CertID         string
	SourceMatch    bool   // working tree matches the lock's source hash
	CertSelfValid  bool   // the stored cert's id matches its own fields
	Reproduces     bool   // a fresh run yields the same equity-curve hash
	LockedEquity   string // equity-curve hash recorded in the cert
	CurrentEquity  string // equity-curve hash from the fresh run
	CurrentSrcHash string
	LockedSrcHash  string
}

// OK is true only when every check passes.
func (v VerifyResult) OK() bool {
	return v.SourceMatch && v.CertSelfValid && v.Reproduces
}

// VerifyBacktest re-runs the strategy and confirms it reproduces the certificate
// bit-for-bit, and that the working tree is the source the cert was built from.
func VerifyBacktest(dir string, opts BacktestOptions) (VerifyResult, error) {
	cert, err := ReadCert(dir)
	if err != nil {
		return VerifyResult{}, fmt.Errorf("%w (run `lumen quant backtest` first)", err)
	}
	lock, err := ReadLock(dir)
	if err != nil {
		return VerifyResult{}, err
	}
	m, err := LoadManifest(dir)
	if err != nil {
		return VerifyResult{}, err
	}
	curSrc, err := ComputeSrcHash(dir)
	if err != nil {
		return VerifyResult{}, err
	}

	res, _, err := runHarness(dir, m, opts)
	if err != nil {
		return VerifyResult{}, err
	}

	return VerifyResult{
		CertID:         cert.ID,
		SourceMatch:    curSrc == lock.SrcHash,
		CertSelfValid:  VerifyCert(cert),
		Reproduces:     res.EquityCurveHash == cert.EquityCurveHash,
		LockedEquity:   cert.EquityCurveHash,
		CurrentEquity:  res.EquityCurveHash,
		CurrentSrcHash: curSrc,
		LockedSrcHash:  lock.SrcHash,
	}, nil
}
