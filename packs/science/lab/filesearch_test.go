package lab

import (
	"os"
	"path/filepath"
	"testing"
)

func TestSearchWorkspaceNameAndContent(t *testing.T) {
	root := t.TempDir()
	_ = os.WriteFile(filepath.Join(root, "readme.md"), []byte("# Hello\naspirin study notes\n"), 0o600)
	_ = os.MkdirAll(filepath.Join(root, "data"), 0o700)
	_ = os.WriteFile(filepath.Join(root, "data", "results.csv"), []byte("a,b\n1,2\n"), 0o600)
	_ = os.WriteFile(filepath.Join(root, "noise.bin"), []byte{0, 1, 2, 3}, 0o600)

	hits, err := SearchWorkspace(root, "aspirin", 50)
	if err != nil {
		t.Fatal(err)
	}
	if len(hits) < 1 {
		t.Fatalf("expected content hit, got %+v", hits)
	}
	found := false
	for _, h := range hits {
		if h.Path == "readme.md" && h.Match == "content" {
			found = true
			if h.Snippet == "" {
				t.Fatal("snippet empty")
			}
		}
	}
	if !found {
		t.Fatalf("no content match: %+v", hits)
	}

	nameHits, err := SearchWorkspace(root, "results", 50)
	if err != nil {
		t.Fatal(err)
	}
	ok := false
	for _, h := range nameHits {
		if h.Name == "results.csv" && h.Match == "name" {
			ok = true
		}
	}
	if !ok {
		t.Fatalf("name hit missing: %+v", nameHits)
	}

	empty, _ := SearchWorkspace(root, "   ", 10)
	if empty != nil && len(empty) != 0 {
		t.Fatalf("empty query: %+v", empty)
	}
}
