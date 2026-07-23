package project

import (
	"os"
	"path/filepath"
	"testing"
)

func TestSlugFromTitle(t *testing.T) {
	if got := SlugFromTitle("EGFR 抑制剂"); got != "egfr" {
		t.Fatalf("slug = %q", got)
	}
}

func TestCreateProject(t *testing.T) {
	dir := t.TempDir()
	sci := filepath.Join(dir, "science")
	store := NewStore(sci)
	p, err := store.Create("Aspirin Study", "")
	if err != nil {
		t.Fatal(err)
	}
	if p.Slug == "" {
		t.Fatal("empty slug")
	}
	ws, err := store.WorkspacePath(p.Slug)
	if err != nil {
		t.Fatal(err)
	}
	if st, err := os.Stat(ws); err != nil || !st.IsDir() {
		t.Fatalf("workspace missing: %v", err)
	}
}

func TestCreateProjectWithSeedTemplate(t *testing.T) {
	dir := t.TempDir()
	sci := filepath.Join(dir, "science")
	seed := filepath.Join(sci, "sandbox", "home", ".claude-science", "seed-assets", "example_crispr_screen")
	if err := os.MkdirAll(seed, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(seed, "README.md"), []byte("seed-readme"), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewStore(sci)
	p, err := store.Create("CRISPR Screen", "example_crispr_screen")
	if err != nil {
		t.Fatal(err)
	}
	ws, err := store.WorkspacePath(p.Slug)
	if err != nil {
		t.Fatal(err)
	}
	got := filepath.Join(ws, "data", "example_crispr_screen", "README.md")
	if st, err := os.Stat(got); err != nil || st.Size() == 0 {
		t.Fatalf("seeded README missing at %s: %v", got, err)
	}
}

func TestCreateChineseTitleUniqueSlug(t *testing.T) {
	s := NewStore(t.TempDir())
	p1, err := s.Create("验收课题", "")
	if err != nil {
		t.Fatal(err)
	}
	p2, err := s.Create("验收课题", "")
	if err != nil {
		t.Fatal(err)
	}
	if p1.Slug == p2.Slug {
		t.Fatalf("slugs should differ: %s", p1.Slug)
	}
	if p1.Title != "验收课题" {
		t.Fatalf("title lost: %s", p1.Title)
	}
}
