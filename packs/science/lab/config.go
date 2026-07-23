package lab

import (
	"encoding/json"
	"os"
	"path/filepath"

	sciconfig "lumen/internal/science/config"
)

const defaultToolProfile = "full_science"

// LocalConfig holds lab-specific settings under ~/.lumen/science/lab/.
type LocalConfig struct {
	DefaultPort int    `json:"default_port,omitempty"`
	ToolProfile string `json:"tool_profile,omitempty"`
	DefaultMode string `json:"default_mode,omitempty"` // plan | agent
}

func labRoot(sciDir string) string {
	return filepath.Join(sciDir, "lab")
}

func localConfigPath(sciDir string) string {
	return filepath.Join(labRoot(sciDir), "lab.json")
}

func loadLocalConfig(sciDir string) LocalConfig {
	// Default agent so tool calls + approval path are exercised (plan never runs tools).
	cfg := LocalConfig{DefaultPort: DefaultPort, ToolProfile: defaultToolProfile, DefaultMode: "agent"}
	data, err := os.ReadFile(localConfigPath(sciDir))
	if err != nil {
		return cfg
	}
	_ = json.Unmarshal(data, &cfg)
	if cfg.DefaultPort == 0 {
		cfg.DefaultPort = DefaultPort
	}
	if cfg.ToolProfile == "" {
		cfg.ToolProfile = defaultToolProfile
	}
	if cfg.DefaultMode == "" {
		cfg.DefaultMode = "agent"
	}
	return cfg
}

func saveLocalConfig(sciDir string, cfg LocalConfig) error {
	if cfg.DefaultPort == 0 {
		cfg.DefaultPort = DefaultPort
	}
	if cfg.ToolProfile == "" {
		cfg.ToolProfile = defaultToolProfile
	}
	if cfg.DefaultMode == "" {
		cfg.DefaultMode = "agent"
	}
	// normalize mode
	switch cfg.DefaultMode {
	case "plan", "agent", "bypass", "default":
	default:
		cfg.DefaultMode = "agent"
	}
	path := localConfigPath(sciDir)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	data, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(data, '\n'), 0o600)
}

func scienceConfig(sciDir string) (sciconfig.File, error) {
	return sciconfig.Load(sciDir)
}
