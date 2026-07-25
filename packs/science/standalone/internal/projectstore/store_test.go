package projectstore

import (
	"os"
	"path/filepath"
	"testing"
)

func TestCreateListClaim(t *testing.T) {
	dir := t.TempDir()
	s := New(dir)
	p, err := s.Create("owner", "title", "question?")
	if err != nil {
		t.Fatal(err)
	}
	if p.Status != "Draft" {
		t.Fatalf("status %s", p.Status)
	}
	list, err := s.List()
	if err != nil || len(list) != 1 {
		t.Fatalf("list: %v %d", err, len(list))
	}
	c, err := s.ProposeClaim(p.ProjectID, "owner", "a claim", "sci")
	if err != nil {
		t.Fatal(err)
	}
	if c.Status != "Proposed" || c.EvidenceNodeID == nil {
		t.Fatalf("claim: %+v", c)
	}
	// durable files
	if _, err := os.Stat(filepath.Join(dir, "projects", p.ProjectID, "project.json")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dir, "projects", p.ProjectID, "claims", c.ClaimID+".json")); err != nil {
		t.Fatal(err)
	}
	// ownership
	if _, err := s.ProposeClaim(p.ProjectID, "other", "x", "x"); err == nil {
		t.Fatal("expected ownership error")
	}
}
