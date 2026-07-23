package runtime

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// ResolvePython returns the operon-mcp conda interpreter from the cloned research
// pack. CS bio-tools MCP servers require the mcp package installed in that env;
// system python3 will fail on import.
func ResolvePython(dataDir string) string {
	if dataDir == "" {
		return "python3"
	}
	candidates := []string{
		filepath.Join(dataDir, "conda", "envs", "operon-mcp", "bin", "python3.13"),
		filepath.Join(dataDir, "conda", "envs", "operon-mcp", "bin", "python3"),
		filepath.Join(dataDir, "conda", "envs", "operon-mcp", "bin", "python"),
		filepath.Join(dataDir, "bin", "python3"),
		filepath.Join(dataDir, "bin", "python"),
	}
	for _, p := range candidates {
		if isExecutable(p) {
			return p
		}
	}
	glob := filepath.Join(dataDir, "conda", "envs", "operon-mcp", "bin", "python*")
	matches, _ := filepath.Glob(glob)
	sort.Strings(matches)
	for _, p := range matches {
		if strings.HasSuffix(p, "-config") {
			continue
		}
		if isExecutable(p) {
			return p
		}
	}
	return "python3"
}

func isExecutable(path string) bool {
	st, err := os.Stat(path)
	return err == nil && !st.IsDir() && st.Mode()&0o111 != 0
}
