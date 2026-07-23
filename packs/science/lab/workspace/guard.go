package workspace

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Guard ensures paths stay within a project workspace root.
type Guard struct {
	root string
}

// NewGuard creates a workspace path guard.
func NewGuard(root string) (*Guard, error) {
	abs, err := filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	return &Guard{root: abs}, nil
}

// Resolve validates and resolves a relative path inside the workspace.
func (g *Guard) Resolve(rel string) (string, error) {
	if g == nil || g.root == "" {
		return "", fmt.Errorf("workspace guard not configured")
	}
	rel = strings.TrimSpace(rel)
	if rel == "" || rel == "." {
		return g.root, nil
	}
	if filepath.IsAbs(rel) {
		return "", fmt.Errorf("absolute paths not allowed")
	}
	clean := filepath.Clean(rel)
	if clean == ".." || strings.HasPrefix(clean, ".."+string(os.PathSeparator)) {
		return "", fmt.Errorf("path escapes workspace")
	}
	abs := filepath.Join(g.root, clean)
	abs, err := filepath.Abs(abs)
	if err != nil {
		return "", err
	}
	if !strings.HasPrefix(abs, g.root+string(os.PathSeparator)) && abs != g.root {
		return "", fmt.Errorf("path escapes workspace")
	}
	if fi, err := os.Lstat(abs); err == nil && fi.Mode()&os.ModeSymlink != 0 {
		return "", fmt.Errorf("symlinks not allowed")
	}
	return abs, nil
}
