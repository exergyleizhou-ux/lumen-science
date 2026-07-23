package oauth

import (
	"os"
	"path/filepath"
	"testing"
)

func TestEnsureVirtualLoginCreates(t *testing.T) {
	dir := t.TempDir()
	sbx := filepath.Join(dir, "sandbox")
	auth := filepath.Join(sbx, ".claude-science")
	real := filepath.Join(dir, "real-cred")
	if err := os.MkdirAll(real, 0o700); err != nil {
		t.Fatal(err)
	}
	fr, action, err := EnsureVirtualLogin(auth, sbx, real)
	if err != nil {
		t.Fatal(err)
	}
	if action != ActionCreated {
		t.Fatalf("action=%s", action)
	}
	if _, err := os.Stat(fr.EncFile); err != nil {
		t.Fatal(err)
	}
	fr2, action2, err := EnsureVirtualLogin(auth, sbx, real)
	if err != nil {
		t.Fatal(err)
	}
	if action2 != ActionReused {
		t.Fatalf("second action=%s", action2)
	}
	if fr2.AccountUUID != fr.AccountUUID {
		t.Fatal("account should be reused")
	}
}

func TestIsLoginIntactAfterEnsure(t *testing.T) {
	dir := t.TempDir()
	sbx := filepath.Join(dir, "sandbox")
	auth := filepath.Join(sbx, ".claude-science")
	real := filepath.Join(dir, "real-cred")
	if err := os.MkdirAll(real, 0o700); err != nil {
		t.Fatal(err)
	}
	if _, _, err := EnsureVirtualLogin(auth, sbx, real); err != nil {
		t.Fatal(err)
	}
	if !IsLoginIntact(auth) {
		t.Fatal("expected intact login after ensure")
	}
}

func TestResolveGuardedRejectsRealDir(t *testing.T) {
	dir := t.TempDir()
	real := filepath.Join(dir, "real")
	if err := os.MkdirAll(real, 0o700); err != nil {
		t.Fatal(err)
	}
	_, err := resolveGuarded(real, virtualEmail, dir, real)
	if err == nil {
		t.Fatal("expected refusal when auth_dir is real cred dir")
	}
}
