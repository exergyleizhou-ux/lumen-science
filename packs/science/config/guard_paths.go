package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// AssertConfigDirIsolated ensures resolved config dir is not inside real Science credential trees.
func AssertConfigDirIsolated(dir string) error {
	if err := assertNotSymlink(dir); err != nil {
		return err
	}
	abs, err := filepath.Abs(dir)
	if err != nil {
		return err
	}
	canon, err := filepath.EvalSymlinks(abs)
	if err != nil {
		canon = abs
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return err
	}
	realHome, err := filepath.EvalSymlinks(home)
	if err != nil {
		realHome = home
	}
	forbidden := []string{
		filepath.Join(realHome, ".claude-science"),
		filepath.Join(realHome, ".claude"),
	}
	for _, f := range forbidden {
		if underPath(canon, f) {
			return fmt.Errorf("refuse config dir inside real Science tree: %s", canon)
		}
	}
	return nil
}

func underPath(path, root string) bool {
	path = filepath.Clean(path)
	root = filepath.Clean(root)
	if path == root {
		return true
	}
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return false
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(os.PathSeparator))
}
