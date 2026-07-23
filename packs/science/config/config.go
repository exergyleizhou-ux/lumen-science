// Package config manages ~/.lumen/science/config.json for the Science bridge.
package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"

	"lumen/internal/science/guard"
	"lumen/internal/science/proxy"
)

const (
	DefaultProvider    = "deepseek"
	DefaultProxyPort   = 18991
	DefaultSandboxPort = 8990
	DefaultMode        = "proxy"
	DefaultScienceMode = "hybrid" // hybrid | native | bridge
)

// ProviderCfg holds one provider API key (0600 file, never log).
type ProviderCfg struct {
	Key string `json:"key,omitempty"`
}

// NativeMCPCfg controls the Lumen-owned MCP fleet.
type NativeMCPCfg struct {
	Fleet     []string `json:"fleet,omitempty"`
	AutoStart bool     `json:"auto_start,omitempty"`
}

// OasisCfg holds 绿洲 marketplace connection (auth combo 1+3).
// Anonymous: search_datasets, get_dataset_detail.
// User OAuth token: preview_schema, certificates, C2D jobs.
type OasisCfg struct {
	BaseURL       string `json:"base_url,omitempty"`
	APIToken      string `json:"api_token,omitempty"`       // user OAuth access token (0600)
	ServiceKeyEnv string `json:"service_key_env,omitempty"` // optional service key env var
}

// File is the persisted science bridge configuration.
type File struct {
	SchemaVersion   int                    `json:"schema_version,omitempty"`
	Profiles        []Profile              `json:"profiles,omitempty"`
	ActiveProfileID string                 `json:"active_profile_id,omitempty"`
	Provider        string                 `json:"provider"`
	ProxyPort       int                    `json:"proxy_port"`
	SandboxPort     int                    `json:"sandbox_port"`
	Secret          string                 `json:"secret,omitempty"`
	Mode            string                 `json:"mode"`                   // "proxy" | "official"
	ScienceMode     string                 `json:"science_mode,omitempty"` // hybrid | native | bridge
	CacheBoost      *bool                  `json:"cache_boost,omitempty"`  // DeepSeek prefix-cache: inject system/tools cache_control
	ToolUseShim     string                 `json:"tooluse_shim,omitempty"` // off | detect | rewrite
	Providers       map[string]ProviderCfg `json:"providers,omitempty"`
	NativeMCP       NativeMCPCfg           `json:"native_mcp,omitempty"`
	Oasis           OasisCfg               `json:"oasis,omitempty"`
}

// Default returns factory defaults.
func Default() File {
	return File{
		Provider:    DefaultProvider,
		ProxyPort:   DefaultProxyPort,
		SandboxPort: DefaultSandboxPort,
		Mode:        DefaultMode,
		Providers:   map[string]ProviderCfg{},
	}
}

// Dir returns ~/.lumen/science.
func Dir() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, ".lumen", "science"), nil
}

func configPath(dir string) string {
	return filepath.Join(dir, "config.json")
}

var writeMu sync.Mutex
var tmpSeq atomic.Uint64

// Load reads config.json; missing file yields defaults.
func Load(dir string) (File, error) {
	f, err := loadRaw(dir)
	if err != nil {
		return File{}, err
	}
	if err := EnsureMigrated(dir, &f); err != nil {
		return File{}, err
	}
	return f, nil
}

func loadRaw(dir string) (File, error) {
	if err := assertNotSymlink(dir); err != nil {
		return File{}, err
	}
	path := configPath(dir)
	if err := assertNotSymlink(path); err != nil {
		return File{}, err
	}
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return Default(), nil
	}
	if err != nil {
		return File{}, err
	}
	_ = os.Chmod(path, 0o600)
	var f File
	if err := json.Unmarshal(data, &f); err != nil {
		return File{}, fmt.Errorf("config.json: %w", err)
	}
	if f.Provider == "" {
		f.Provider = DefaultProvider
	}
	if f.ProxyPort == 0 {
		f.ProxyPort = DefaultProxyPort
	}
	if f.SandboxPort == 0 {
		f.SandboxPort = DefaultSandboxPort
	}
	if f.Mode == "" {
		f.Mode = DefaultMode
	}
	if f.ScienceMode == "" {
		f.ScienceMode = DefaultScienceMode
	}
	if f.Providers == nil {
		f.Providers = map[string]ProviderCfg{}
	}
	if f.SchemaVersion == 0 {
		f.SchemaVersion = 1
	}
	return f, nil
}

// Save atomically writes config.json with 0600 permissions.
func Save(dir string, f File) error {
	writeMu.Lock()
	defer writeMu.Unlock()
	if err := ensureDir(dir); err != nil {
		return err
	}
	path := configPath(dir)
	if err := assertNotSymlink(path); err != nil {
		return err
	}
	data, err := json.MarshalIndent(f, "", "  ")
	if err != nil {
		return err
	}
	// Extreme atomic write: pid + seq + O_EXCL create_new + 0600 at open + Sync + rename + reset perm.
	seq := tmpSeq.Add(1)
	tmp := filepath.Join(dir, fmt.Sprintf(".config.json.tmp-%d-%d", os.Getpid(), seq))
	writeRes := func() error {
		fh, err := os.OpenFile(tmp, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
		if err != nil {
			return err
		}
		if _, err := fh.Write(append(data, '\n')); err != nil {
			fh.Close()
			return err
		}
		if err := fh.Sync(); err != nil {
			fh.Close()
			return err
		}
		return fh.Close()
	}()
	if writeRes != nil {
		_ = os.Remove(tmp)
		return writeRes
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return os.Chmod(path, 0o600)
}

// Validate checks ports, provider, and mode before persisting.
func Validate(f File) error {
	if f.Provider != "" {
		// Schema v2 profiles use adapter names (e.g. relay) in the legacy Provider slot.
		if f.Provider != "relay" {
			if _, ok := proxy.LookupProvider(f.Provider); !ok {
				return fmt.Errorf("unsupported provider %q", f.Provider)
			}
		}
	}
	if err := guard.AssertPortSafe(f.ProxyPort); err != nil {
		return err
	}
	if err := guard.AssertPortSafe(f.SandboxPort); err != nil {
		return err
	}
	if err := guard.AssertPortsDistinct(f.ProxyPort, f.SandboxPort); err != nil {
		return err
	}
	if f.Mode != "" && f.Mode != "proxy" && f.Mode != "official" {
		return fmt.Errorf("mode must be proxy or official")
	}
	switch f.ScienceMode {
	case "", "hybrid", "native", "bridge":
	default:
		return fmt.Errorf("science_mode must be hybrid, native, or bridge")
	}
	switch strings.ToLower(strings.TrimSpace(f.ToolUseShim)) {
	case "", "off", "detect", "rewrite":
	default:
		return fmt.Errorf("tooluse_shim must be off, detect, or rewrite")
	}
	if f.SchemaVersion > CurrentSchemaVersion {
		return fmt.Errorf("config schema v%d is newer than this binary supports (v%d)", f.SchemaVersion, CurrentSchemaVersion)
	}
	return nil
}

// Update loads, applies fn, saves.
func Update(dir string, fn func(*File)) (File, error) {
	writeMu.Lock()
	defer writeMu.Unlock()
	f, err := loadRaw(dir)
	if err != nil {
		return File{}, err
	}
	if f.SchemaVersion < CurrentSchemaVersion {
		migrated := applyMigration(f)
		if err := persistMigration(dir, migrated); err != nil {
			return File{}, err
		}
		f = migrated
	}
	fn(&f)
	if err := Validate(f); err != nil {
		return File{}, err
	}
	if err := saveUnlocked(dir, f); err != nil {
		return File{}, err
	}
	return f, nil
}

func saveUnlocked(dir string, f File) error {
	if err := ensureDir(dir); err != nil {
		return err
	}
	path := configPath(dir)
	data, err := json.MarshalIndent(f, "", "  ")
	if err != nil {
		return err
	}
	// Extreme atomic write: pid + seq + O_EXCL create_new + 0600 at open + Sync + rename + reset perm.
	seq := tmpSeq.Add(1)
	tmp := filepath.Join(dir, fmt.Sprintf(".config.json.tmp-%d-%d", os.Getpid(), seq))
	writeRes := func() error {
		fh, err := os.OpenFile(tmp, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
		if err != nil {
			return err
		}
		if _, err := fh.Write(append(data, '\n')); err != nil {
			fh.Close()
			return err
		}
		if err := fh.Sync(); err != nil {
			fh.Close()
			return err
		}
		return fh.Close()
	}()
	if writeRes != nil {
		_ = os.Remove(tmp)
		return writeRes
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return os.Chmod(path, 0o600)
}

// CacheBoostEnabled returns whether to inject cache_control on system/tools (default true for deepseek).
func (f File) CacheBoostEnabled() bool {
	if f.CacheBoost != nil {
		return *f.CacheBoost
	}
	return strings.EqualFold(f.Provider, "deepseek")
}

// KeyFor returns the stored key for a provider name.
func (f File) KeyFor(provider string) string {
	if p, ok := f.Providers[provider]; ok {
		return p.Key
	}
	return ""
}

// OasisBaseURL returns the marketplace base URL with a demo default.
func (f File) OasisBaseURL() string {
	if strings.TrimSpace(f.Oasis.BaseURL) != "" {
		return strings.TrimRight(strings.TrimSpace(f.Oasis.BaseURL), "/")
	}
	return "https://demo.oasisdata2026.xyz"
}

// OasisToken returns the stored user OAuth access token (may be empty).
func (f File) OasisToken() string {
	return f.Oasis.APIToken
}

// MaskKey returns last-4 masked key for display.
func MaskKey(key string) string {
	n := len(key)
	if n == 0 {
		return ""
	}
	if n <= 4 {
		return strings.Repeat("•", n)
	}
	return strings.Repeat("•", n-4) + key[n-4:]
}

func ensureDir(dir string) error {
	if err := AssertConfigDirIsolated(dir); err != nil {
		return err
	}
	if err := assertNotSymlink(dir); err != nil {
		return err
	}
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	// After create, verify is dir (not file) + explicit 0700 reset.
	md, err := os.Stat(dir)
	if err != nil {
		return err
	}
	if !md.IsDir() {
		return fmt.Errorf("config dir is not a directory: %s", dir)
	}
	return os.Chmod(dir, 0o700)
}

func assertNotSymlink(path string) error {
	fi, err := os.Lstat(path)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}
	if fi.Mode()&os.ModeSymlink != 0 {
		return fmt.Errorf("refuse symlink: %s", path)
	}
	return nil
}
