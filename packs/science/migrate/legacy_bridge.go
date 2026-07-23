// Package migrate imports legacy bridge settings into Lumen science config.
package migrate

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/guard"
	"lumen/internal/science/proxy"
)

const legacyBridgeDirName = ".csswitch"

// legacyBridgeConfig mirrors the legacy bridge config.json (read-only import source).
type legacyBridgeConfig struct {
	Provider    string                        `json:"provider"`
	ProxyPort   int                           `json:"proxy_port"`
	SandboxPort int                           `json:"sandbox_port"`
	Secret      string                        `json:"secret,omitempty"`
	Mode        string                        `json:"mode"`
	Providers   map[string]legacyProviderSlot `json:"providers"`
}

type legacyProviderSlot struct {
	Key string `json:"key,omitempty"`
}

// Detect reports whether a legacy bridge config exists and ports are in use.
func Detect() (configPath string, exists bool, portsBusy bool) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", false, false
	}
	path := filepath.Join(home, legacyBridgeDirName, "config.json")
	if _, err := os.Stat(path); err != nil {
		return path, false, false
	}
	cfg, err := readLegacyBridgeConfig(path)
	if err != nil {
		return path, true, false
	}
	busy := guard.PortInUse(cfg.ProxyPort) || guard.PortInUse(cfg.SandboxPort)
	return path, true, busy
}

// Import merges legacy bridge settings into ~/.lumen/science/config.json.
func Import(sciDir string, force bool) (Report, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return Report{}, err
	}
	srcPath := filepath.Join(home, legacyBridgeDirName, "config.json")
	src, err := readLegacyBridgeConfig(srcPath)
	if err != nil {
		return Report{}, fmt.Errorf("read legacy config: %w", err)
	}
	if !force && (guard.PortInUse(src.ProxyPort) || guard.PortInUse(src.SandboxPort)) {
		return Report{}, fmt.Errorf("legacy bridge ports still in use (%d/%d) — stop the other process first",
			src.ProxyPort, src.SandboxPort)
	}

	var rep Report
	rep.Source = srcPath

	_, err = sciconfig.Update(sciDir, func(c *sciconfig.File) {
		if p := strings.ToLower(strings.TrimSpace(src.Provider)); p != "" {
			if _, ok := proxy.LookupProvider(p); ok {
				c.Provider = p
				rep.Provider = p
			}
		}
		if src.ProxyPort > 0 {
			c.ProxyPort = src.ProxyPort
			rep.ProxyPort = src.ProxyPort
		}
		if src.SandboxPort > 0 {
			c.SandboxPort = src.SandboxPort
			rep.SandboxPort = src.SandboxPort
		}
		if m := strings.TrimSpace(src.Mode); m == "proxy" || m == "official" {
			c.Mode = m
			rep.Mode = m
		}
		if c.Secret == "" && strings.TrimSpace(src.Secret) != "" {
			c.Secret = strings.TrimSpace(src.Secret)
			rep.SecretImported = true
		}
		if c.Providers == nil {
			c.Providers = map[string]sciconfig.ProviderCfg{}
		}
		for name, p := range src.Providers {
			k := strings.TrimSpace(p.Key)
			if k == "" {
				continue
			}
			n := strings.ToLower(name)
			if _, ok := proxy.LookupProvider(n); !ok {
				continue
			}
			if existing := c.Providers[n].Key; existing != "" && existing != k && !force {
				continue
			}
			c.Providers[n] = sciconfig.ProviderCfg{Key: k}
			rep.KeysImported = append(rep.KeysImported, n)
		}
	})
	if err != nil {
		return rep, err
	}
	if imported, err := sciconfig.ImportLegacyProfilesV2(sciDir); err != nil {
		return rep, err
	} else if imported {
		rep.V2ProfilesImported = true
	}
	rep.Imported = true
	return rep, nil
}

// Report summarizes a migration run.
type Report struct {
	Imported           bool     `json:"imported"`
	Source             string   `json:"source"`
	Provider           string   `json:"provider,omitempty"`
	ProxyPort          int      `json:"proxy_port,omitempty"`
	SandboxPort        int      `json:"sandbox_port,omitempty"`
	Mode               string   `json:"mode,omitempty"`
	SecretImported     bool     `json:"secret_imported"`
	KeysImported       []string `json:"keys_imported,omitempty"`
	V2ProfilesImported bool     `json:"v2_profiles_imported,omitempty"`
}

func readLegacyBridgeConfig(path string) (legacyBridgeConfig, error) {
	fi, err := os.Lstat(path)
	if err != nil {
		return legacyBridgeConfig{}, err
	}
	if fi.Mode()&os.ModeSymlink != 0 {
		return legacyBridgeConfig{}, fmt.Errorf("refuse symlink: %s", path)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return legacyBridgeConfig{}, err
	}
	var c legacyBridgeConfig
	if err := json.Unmarshal(data, &c); err != nil {
		return legacyBridgeConfig{}, err
	}
	if c.Providers == nil {
		c.Providers = map[string]legacyProviderSlot{}
	}
	return c, nil
}
