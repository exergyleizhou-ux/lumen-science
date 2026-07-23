package runtime

import (
	"os"
	"path/filepath"

	"lumen/internal/science/paths"
)

// CondaEnv returns the dataDir/bin and dataDir/conda paths for the lab agent's
// bash execution environment.
func CondaEnv(sciDir string) []string {
	dataDir := paths.DataDir(sciDir)
	bin := filepath.Join(dataDir, "bin")
	condaBin := filepath.Join(dataDir, "conda", "bin")
	var paths []string
	for _, p := range []string{bin, condaBin} {
		if _, err := os.Stat(p); err == nil {
			paths = append(paths, p)
		}
	}
	// Also check conda envs/operon-mcp/bin
	operon := filepath.Join(dataDir, "conda", "envs", "operon-mcp", "bin")
	if _, err := os.Stat(operon); err == nil {
		paths = append(paths, operon)
	}
	return paths
}

// Prepends conda/bin directories to PATH for the lab agent's bash tool.
func InjectLabPath(sciDir string) {
	paths := CondaEnv(sciDir)
	if len(paths) == 0 {
		return
	}
	cur := os.Getenv("PATH")
	for _, p := range paths {
		cur = p + string(os.PathListSeparator) + cur
	}
	os.Setenv("PATH", cur)
}
