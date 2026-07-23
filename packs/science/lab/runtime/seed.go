package runtime

import (
	"io"
	"os"
	"path/filepath"
	"strings"

	"lumen/internal/science/paths"
	"lumen/internal/science/research"
)

// SeedExampleDir returns the on-disk path for a named seed example, if present.
func SeedExampleDir(sciDir, name string) string {
	name = strings.TrimSpace(name)
	if name == "" {
		return ""
	}
	for _, candidate := range research.SeedExamples {
		if candidate != name {
			continue
		}
		dataDir := paths.DataDir(sciDir)
		for _, base := range []string{
			filepath.Join(dataDir, "seed-assets", name),
			seedRuntimeDir(dataDir, name),
		} {
			if st, err := os.Stat(base); err == nil && st.IsDir() {
				return base
			}
		}
	}
	return ""
}

func seedRuntimeDir(dataDir, name string) string {
	rtRoot, _, err := findRuntimeRoot(dataDir)
	if err != nil {
		return ""
	}
	return filepath.Join(rtRoot, "seed", name)
}

// CopySeedExample copies a research-pack seed example into workspace/data/<name>.
func CopySeedExample(sciDir, name, workspace string) error {
	src := SeedExampleDir(sciDir, name)
	if src == "" {
		return nil
	}
	dst := filepath.Join(workspace, "data", name)
	return copyTree(src, dst)
}

func copyTree(src, dst string) error {
	return filepath.Walk(src, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		if strings.HasPrefix(rel, ".extracted-") {
			if info.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		target := filepath.Join(dst, rel)
		if info.IsDir() {
			return os.MkdirAll(target, 0o700)
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
			return err
		}
		return copyFile(path, target, info.Mode())
	})
}

func copyFile(src, dst string, mode os.FileMode) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.OpenFile(dst, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, mode.Perm())
	if err != nil {
		return err
	}
	defer out.Close()
	_, err = io.Copy(out, in)
	return err
}
