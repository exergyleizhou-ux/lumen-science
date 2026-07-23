package paths

import (
	"path/filepath"

	sciconfig "lumen/internal/science/config"
)

// SandboxHome is ~/.lumen/science/sandbox/home.
func SandboxHome(sciDir string) string {
	return filepath.Join(sciDir, "sandbox", "home")
}

// DataDir is the sandbox Claude Science data directory.
func DataDir(sciDir string) string {
	return filepath.Join(SandboxHome(sciDir), ".claude-science")
}

// ProxyLog returns the proxy log path.
func ProxyLog(sciDir string) string {
	return filepath.Join(sciDir, "proxy.log")
}

// SandboxLog returns the sandbox log path.
func SandboxLog(sciDir string) string {
	return filepath.Join(sciDir, "sandbox.log")
}

// StateFile returns persisted runtime state path.
func StateFile(sciDir string) string {
	return filepath.Join(sciDir, "state.json")
}

// DefaultDir resolves ~/.lumen/science, creating if needed.
func DefaultDir() (string, error) {
	return sciconfig.Dir()
}
