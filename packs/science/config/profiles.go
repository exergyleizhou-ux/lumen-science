package config

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

const CurrentSchemaVersion = 2

// Profile is a named provider configuration with upstream probe on switch.
type Profile struct {
	ID           string `json:"id"`
	Name         string `json:"name"`
	TemplateID   string `json:"template_id"`
	BaseURL      string `json:"base_url,omitempty"`
	APIKey       string `json:"api_key,omitempty"`
	Model        string `json:"model,omitempty"`
	CreatedAt    int64  `json:"created_at,omitempty"`
	Notes        string `json:"notes,omitempty"`
	Icon         string `json:"icon,omitempty"`
	IconColor    string `json:"icon_color,omitempty"`
	Verified     bool   `json:"verified,omitempty"`
	VerifiedHint string `json:"verified_hint,omitempty"`
}

// NewProfileID returns a random 32-hex id.
func NewProfileID() string {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		return fmt.Sprintf("%032x", time.Now().UnixNano())
	}
	return hex.EncodeToString(b)
}

// ActiveProfile returns the currently active profile or nil.
func (f *File) ActiveProfile() *Profile {
	if f.ActiveProfileID == "" {
		return nil
	}
	for i := range f.Profiles {
		if f.Profiles[i].ID == f.ActiveProfileID {
			return &f.Profiles[i]
		}
	}
	return nil
}

// ProfileByID finds a profile by id.
func (f *File) ProfileByID(id string) *Profile {
	for i := range f.Profiles {
		if f.Profiles[i].ID == id {
			return &f.Profiles[i]
		}
	}
	return nil
}

// EffectiveProvider returns the adapter name for runtime (legacy or profile).
func (f *File) EffectiveProvider() string {
	if p := f.ActiveProfile(); p != nil {
		if tpl, ok := TemplateByID(p.TemplateID); ok {
			return tpl.Adapter
		}
		return "relay"
	}
	return f.Provider
}

// applyMigration upgrades legacy v1 → v2 in memory (no I/O).
func applyMigration(f File) File {
	if f.SchemaVersion >= CurrentSchemaVersion {
		return f
	}
	if len(f.Profiles) > 0 {
		f.SchemaVersion = CurrentSchemaVersion
		return f
	}
	out := migrateV1ToV2(f)
	out.SchemaVersion = CurrentSchemaVersion
	return out
}

// persistMigration saves a v1→v2 migration with backup. Caller must hold writeMu.
func persistMigration(dir string, migrated File) error {
	path := configPath(dir)
	if data, err := os.ReadFile(path); err == nil && len(data) > 0 {
		bak := path + ".v1.bak"
		_ = os.WriteFile(bak, data, 0o600)
	}
	return saveUnlocked(dir, migrated)
}

// EnsureMigrated upgrades legacy v1 config to schema v2 profiles on load (standalone).
func EnsureMigrated(dir string, f *File) error {
	if f.SchemaVersion >= CurrentSchemaVersion {
		return nil
	}
	migrated := applyMigration(*f)
	if migrated.SchemaVersion == f.SchemaVersion && len(migrated.Profiles) == len(f.Profiles) {
		return nil
	}
	writeMu.Lock()
	defer writeMu.Unlock()
	if err := persistMigration(dir, migrated); err != nil {
		return err
	}
	*f = migrated
	return nil
}

func migrateV1ToV2(legacy File) File {
	ts := time.Now().UnixMilli()
	var profiles []Profile
	activeID := ""
	for slot, pc := range legacy.Providers {
		k := strings.TrimSpace(pc.Key)
		if k == "" {
			continue
		}
		tid := TemplateIDForLegacySlot(slot)
		tpl, _ := TemplateByID(tid)
		id := NewProfileID()
		baseURL := tpl.BaseURL
		profiles = append(profiles, Profile{
			ID: id, Name: tpl.Name, TemplateID: tid, BaseURL: baseURL,
			APIKey: k, CreatedAt: ts,
		})
		if strings.EqualFold(slot, legacy.Provider) {
			activeID = id
		}
	}
	// Legacy single-slot without Providers map
	if len(profiles) == 0 && legacy.Provider != "" {
		if k := legacy.KeyFor(legacy.Provider); k != "" {
			tid := TemplateIDForLegacySlot(legacy.Provider)
			tpl, _ := TemplateByID(tid)
			id := NewProfileID()
			profiles = append(profiles, Profile{
				ID: id, Name: tpl.Name, TemplateID: tid, BaseURL: tpl.BaseURL,
				APIKey: k, CreatedAt: ts,
			})
			activeID = id
		}
	}
	out := legacy
	out.Profiles = profiles
	out.ActiveProfileID = activeID
	return out
}

// SaveWithBackup atomically saves with a rolling .bak before overwrite.
func SaveWithBackup(dir string, f File) error {
	path := configPath(dir)
	if data, err := os.ReadFile(path); err == nil && len(data) > 0 {
		_ = os.WriteFile(path+".bak", data, 0o600)
	}
	return Save(dir, f)
}

// LoadProfilesFile reads config and ensures schema v2 migration.
func LoadProfilesFile(dir string) (File, error) {
	f, err := Load(dir)
	if err != nil {
		return File{}, err
	}
	if err := EnsureMigrated(dir, &f); err != nil {
		return File{}, err
	}
	return f, nil
}

// ImportLegacyProfilesV2 merges legacy v2 multi-profile config from the legacy bridge dir when present.
func ImportLegacyProfilesV2(sciDir string) (bool, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return false, err
	}
	src := filepath.Join(home, ".csswitch", "config.json")
	data, err := os.ReadFile(src)
	if err != nil {
		return false, nil
	}
	var raw struct {
		SchemaVersion int `json:"schema_version"`
		Profiles      []Profile
		ActiveID      string `json:"active_id"`
		ProxyPort     int    `json:"proxy_port"`
		SandboxPort   int    `json:"sandbox_port"`
		Secret        string `json:"secret"`
		Mode          string `json:"mode"`
	}
	if json.Unmarshal(data, &raw) != nil || raw.SchemaVersion < 2 || len(raw.Profiles) == 0 {
		return false, nil
	}
	_, err = Update(sciDir, func(c *File) {
		c.SchemaVersion = CurrentSchemaVersion
		c.Profiles = raw.Profiles
		c.ActiveProfileID = raw.ActiveID
		if raw.ProxyPort > 0 {
			c.ProxyPort = raw.ProxyPort
		}
		if raw.SandboxPort > 0 {
			c.SandboxPort = raw.SandboxPort
		}
		if c.Secret == "" && raw.Secret != "" {
			c.Secret = raw.Secret
		}
		if raw.Mode != "" {
			c.Mode = raw.Mode
		}
		if p := c.ActiveProfile(); p != nil {
			if tpl, ok := TemplateByID(p.TemplateID); ok {
				c.Provider = tpl.Adapter
			}
		}
	})
	return true, err
}
