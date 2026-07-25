package pipeline

import (
	"os"
	"path/filepath"
	"testing"
)

func TestRunSeqOffline(t *testing.T) {
	root := t.TempDir()
	src := []byte(">demo\nATGAATTCGCGGCCGCAAAAAAAAAAAAAAAAATAA\n")
	res, err := RunSeqOffline(root, "proj1", "run1", "demo.fa", src)
	if err != nil {
		t.Fatal(err)
	}
	if res.Review.Status != "pass" {
		t.Fatalf("review failed: %+v", res.Review)
	}
	if res.SourceArtifact.SHA256 == "" || res.AnalysisArtifact.SHA256 == "" {
		t.Fatal("missing hashes")
	}
	// files exist
	p := filepath.Join(root, "proj1", "run1", "derived", "report.md")
	if _, err := os.Stat(p); err != nil {
		t.Fatal(err)
	}
}

func TestEmptyFails(t *testing.T) {
	_, err := RunSeqOffline(t.TempDir(), "p", "r", "x.fa", nil)
	if err == nil {
		t.Fatal("expected error")
	}
}
