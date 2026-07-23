package migrate

import (
	"os"
	"path/filepath"
	"testing"
)

func TestImportFromLegacyBridge(t *testing.T) {
	home := t.TempDir()
	legacyDir := filepath.Join(home, ".csswitch")
	sciDir := filepath.Join(home, ".lumen", "science")
	if err := os.MkdirAll(legacyDir, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(sciDir, 0o700); err != nil {
		t.Fatal(err)
	}
	cfg := `{
  "provider": "deepseek",
  "proxy_port": 18991,
  "sandbox_port": 8990,
  "mode": "proxy",
  "secret": "test-secret-abc",
  "providers": {"deepseek": {"key": "sk-test-deepseek"}}
}`
	if err := os.WriteFile(filepath.Join(legacyDir, "config.json"), []byte(cfg), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("HOME", home)

	rep, err := Import(sciDir, true)
	if err != nil {
		t.Fatal(err)
	}
	if !rep.Imported || rep.Provider != "deepseek" {
		t.Fatalf("rep=%+v", rep)
	}
	if len(rep.KeysImported) != 1 || rep.KeysImported[0] != "deepseek" {
		t.Fatalf("keys=%v", rep.KeysImported)
	}
}
