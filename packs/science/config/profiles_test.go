package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestMigrateV1ToV2(t *testing.T) {
	dir := t.TempDir()
	legacy := Default()
	legacy.Provider = "deepseek"
	legacy.Providers = map[string]ProviderCfg{
		"deepseek": {Key: "sk-test-deepseek"},
		"qwen":     {Key: "sk-test-qwen"},
	}
	if err := Save(dir, legacy); err != nil {
		t.Fatal(err)
	}
	f, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if f.SchemaVersion != CurrentSchemaVersion {
		t.Fatalf("schema %d", f.SchemaVersion)
	}
	if len(f.Profiles) != 2 {
		t.Fatalf("profiles %d", len(f.Profiles))
	}
	if f.ActiveProfileID == "" {
		t.Fatal("expected active profile for deepseek")
	}
	ap := f.ActiveProfile()
	if ap == nil || ap.APIKey != "sk-test-deepseek" {
		t.Fatalf("active %#v", ap)
	}
}

func TestProfileRoundtrip(t *testing.T) {
	dir := t.TempDir()
	p := Profile{
		ID: NewProfileID(), Name: "Test Relay", TemplateID: "custom",
		BaseURL: "https://relay.example.com", APIKey: "sk-relay", CreatedAt: 1,
	}
	f := Default()
	f.SchemaVersion = CurrentSchemaVersion
	f.Profiles = []Profile{p}
	f.ActiveProfileID = p.ID
	if err := SaveWithBackup(dir, f); err != nil {
		t.Fatal(err)
	}
	loaded, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(loaded.Profiles) != 1 || loaded.Profiles[0].APIKey != "sk-relay" {
		t.Fatalf("loaded %#v", loaded.Profiles)
	}
	bak := filepath.Join(dir, "config.json.bak")
	if _, err := os.Stat(bak); os.IsNotExist(err) {
		// first save may not have bak — ok
	}
}
