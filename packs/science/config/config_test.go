package config

import (
	"os"
	"testing"
)

func TestValidatePorts(t *testing.T) {
	cases := []struct {
		name    string
		f       File
		wantErr bool
	}{
		{"defaults ok", Default(), false},
		{"reserved real port", File{Provider: "deepseek", ProxyPort: 8765, SandboxPort: 8990, Mode: "proxy"}, true},
		{"same ports", File{Provider: "deepseek", ProxyPort: 18991, SandboxPort: 18991, Mode: "proxy"}, true},
		{"bad provider", File{Provider: "bogus", ProxyPort: 18991, SandboxPort: 8990, Mode: "proxy"}, true},
		{"relay profile slot", File{SchemaVersion: CurrentSchemaVersion, Provider: "relay", ProxyPort: 18991, SandboxPort: 8990, Mode: "proxy"}, false},
		{"bad mode", File{Provider: "deepseek", ProxyPort: 18991, SandboxPort: 8990, Mode: "hybrid"}, true},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			err := Validate(c.f)
			if (err != nil) != c.wantErr {
				t.Fatalf("Validate() err=%v wantErr=%v", err, c.wantErr)
			}
		})
	}
}

func TestUpdateRejectsInvalidPorts(t *testing.T) {
	dir := t.TempDir()
	if _, err := Update(dir, func(c *File) {
		c.ProxyPort = 8765
	}); err == nil {
		t.Fatal("expected error for reserved port")
	}
}

func TestLoadSaveRoundtripAndPerms(t *testing.T) {
	dir := t.TempDir()
	f := Default()
	f.Provider = "deepseek"
	f.ProxyPort = 18991
	f.Secret = "sekret"
	if err := Save(dir, f); err != nil {
		t.Fatalf("save: %v", err)
	}
	loaded, err := Load(dir)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if loaded.Secret != "sekret" || loaded.ProxyPort != 18991 {
		t.Fatalf("roundtrip mismatch: %+v", loaded)
	}
	// after save/load, file should be 0600
	path := configPath(dir)
	fi, _ := os.Stat(path)
	if fi.Mode().Perm() != 0o600 {
		t.Errorf("config file perm = %o want 0600", fi.Mode().Perm())
	}
}

func TestSymlinkRejection(t *testing.T) {
	base := t.TempDir()
	real := base + "/real"
	if err := os.MkdirAll(real, 0o700); err != nil {
		t.Fatal(err)
	}
	link := base + "/.lumen-science-link"
	if err := os.Symlink(real, link); err != nil {
		t.Fatal(err)
	}
	// Load on symlinked dir must refuse
	if _, err := Load(link); err == nil {
		t.Error("expected Load to reject symlinked dir")
	}
	// ensure via Save should refuse too
	if err := Save(link, Default()); err == nil {
		t.Error("expected Save to reject symlinked dir")
	}
}

func TestEnsureDirAndPermResetOnLoad(t *testing.T) {
	dir := t.TempDir()
	// force a 0644 file then Load should reset to 0600
	p := configPath(dir)
	_ = os.MkdirAll(dir, 0o700)
	if err := os.WriteFile(p, []byte(`{"provider":"deepseek"}`), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err := Load(dir)
	if err != nil {
		t.Fatalf("load after wide perm: %v", err)
	}
	fi, _ := os.Stat(p)
	if fi.Mode().Perm() != 0o600 {
		t.Errorf("load must reset to 0600, got %o", fi.Mode().Perm())
	}
}
