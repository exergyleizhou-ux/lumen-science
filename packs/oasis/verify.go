package oasis

// VerifyResult reports whether a working tree still matches its provenance lock.
type VerifyResult struct {
	SourceMatch bool
	CurrentHash string // recomputed source hash of the working tree
	LockedHash  string // source_sha256 recorded at build time
	Digest      string // the locked (deployed) image digest, for display
}

// VerifySource recomputes the source hash of dir and compares it to the
// lockfile's recorded source_sha256 — confirming this working tree is the exact
// source that produced the locked (and deployed) artifact. A mismatch means the
// source drifted after build/deploy, so the locked image digest no longer
// corresponds to the code in front of you.
func VerifySource(dir string) (VerifyResult, error) {
	lock, err := ReadLock(dir)
	if err != nil {
		return VerifyResult{}, err
	}
	cur, err := ComputeSrcHash(dir)
	if err != nil {
		return VerifyResult{}, err
	}
	return VerifyResult{
		SourceMatch: cur == lock.SrcHash,
		CurrentHash: cur,
		LockedHash:  lock.SrcHash,
		Digest:      lock.Digest,
	}, nil
}
