// Package guard enforces iron rules for Claude Science sandbox operations.
package guard

import (
	"fmt"
	"net"
	"os"
	"path/filepath"
	"strings"
)

const (
	RealInstancePort  = 8765
	DefaultScienceBin = "/Applications/Claude Science.app/Contents/Resources/bin/claude-science"
)

// RealScienceDir returns the user's real Claude Science data directory.
// When HOME is overridden (guard runs), set SCIENCE_REAL_HOME to the actual user home
// so asset clone reads ~/.claude-science without writing there.
func RealScienceDir() (string, error) {
	home := strings.TrimSpace(os.Getenv("SCIENCE_REAL_HOME"))
	if home == "" {
		var err error
		home, err = os.UserHomeDir()
		if err != nil {
			return "", err
		}
	}
	return filepath.Join(home, ".claude-science"), nil
}

// AssertPortSafe rejects the real instance port 8765.
func AssertPortSafe(port int) error {
	if port == RealInstancePort {
		return fmt.Errorf("port %d is reserved for the real Science instance", RealInstancePort)
	}
	if port <= 0 || port > 65535 {
		return fmt.Errorf("invalid port %d", port)
	}
	return nil
}

// AssertDataDirIsolated ensures sandbox data-dir is not the real directory.
func AssertDataDirIsolated(dataDir, realDir string) error {
	dataAbs, err := filepath.Abs(dataDir)
	if err != nil {
		return err
	}
	realAbs, err := filepath.Abs(realDir)
	if err != nil {
		return err
	}
	if dataAbs == realAbs {
		return fmt.Errorf("refuse: data-dir points to real Science directory")
	}
	return nil
}

// AssertPortsDistinct ensures proxy and sandbox use different ports.
func AssertPortsDistinct(proxyPort, sandboxPort int) error {
	if proxyPort == sandboxPort {
		return fmt.Errorf("proxy port and sandbox port must differ")
	}
	return nil
}

// ScienceBin resolves the claude-science binary path.
func ScienceBin() string {
	if v := os.Getenv("SCIENCE_BIN"); v != "" {
		return v
	}
	return DefaultScienceBin
}

// PortInUse reports whether something is listening on the port.
func PortInUse(port int) bool {
	ln, err := net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", port))
	if err != nil {
		return true
	}
	ln.Close()
	return false
}

// ScienceBinExists reports whether the Science CLI binary is present.
func ScienceBinExists() bool {
	fi, err := os.Stat(ScienceBin())
	return err == nil && fi.Mode()&0o111 != 0
}
