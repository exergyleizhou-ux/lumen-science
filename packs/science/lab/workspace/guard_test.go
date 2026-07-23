package workspace

import (
	"path/filepath"
	"testing"
)

func TestGuardRejectsDotDot(t *testing.T) {
	root := t.TempDir()
	g, err := NewGuard(root)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := g.Resolve("../etc/passwd"); err == nil {
		t.Fatal("expected reject")
	}
	abs, err := g.Resolve("reports/brief.md")
	if err != nil {
		t.Fatal(err)
	}
	if filepath.Base(abs) != "brief.md" {
		t.Fatalf("got %s", abs)
	}
}
