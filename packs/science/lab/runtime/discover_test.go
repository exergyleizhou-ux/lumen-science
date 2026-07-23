package runtime

import (
	"path/filepath"
	"testing"
)

func TestDiscoverMissingPack(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "science")
	_, _, err := Discover(dir)
	if err == nil {
		t.Fatal("expected error without research pack")
	}
}

func TestDomainPackage(t *testing.T) {
	if got := domainPackage("clinical-trials"); got != "mcp_clinical_trials" {
		t.Fatalf("got %q", got)
	}
}
