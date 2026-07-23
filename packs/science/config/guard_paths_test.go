package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestAssertConfigDirIsolatedRejectsScienceTree(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Fatal(err)
	}
	bad := filepath.Join(home, ".claude-science", "nested")
	if err := AssertConfigDirIsolated(bad); err == nil {
		t.Fatal("expected reject inside .claude-science")
	}
}

func TestAssertConfigDirIsolatedAllowsTemp(t *testing.T) {
	dir := t.TempDir()
	if err := AssertConfigDirIsolated(dir); err != nil {
		t.Fatal(err)
	}
}

func TestAssertConfigDirIsolatedRejectsSymlink(t *testing.T) {
	dir := t.TempDir()
	link := filepath.Join(dir, "link")
	target := filepath.Join(dir, "target")
	if err := os.Mkdir(target, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(target, link); err != nil {
		t.Skip("symlink not permitted")
	}
	if err := AssertConfigDirIsolated(link); err == nil {
		t.Fatal("expected symlink reject")
	}
}
