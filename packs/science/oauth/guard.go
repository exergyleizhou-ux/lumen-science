package oauth

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

func resolveGuarded(authDir, email, sandboxRoot, realCredDir string) (string, error) {
	resolved := realAncestor(authDir)
	realRoot := realAncestor(realCredDir)
	if strings.HasPrefix(resolved+string(filepath.Separator), realRoot+string(filepath.Separator)) || resolved == realRoot {
		return "", fmt.Errorf("refuse: auth_dir resolves inside real Science directory %s", realRoot)
	}
	root := realAncestor(sandboxRoot)
	if !strings.HasPrefix(resolved+string(filepath.Separator), root+string(filepath.Separator)) && resolved != root {
		return "", fmt.Errorf("refuse: auth_dir %s is outside sandbox root %s", resolved, root)
	}
	if !strings.HasSuffix(email, "localhost.invalid") {
		return "", fmt.Errorf("refuse: email must end with localhost.invalid (got %s)", email)
	}
	return resolved, nil
}

func realAncestor(p string) string {
	cur := filepath.Clean(p)
	var tail []string
	for {
		if _, err := os.Lstat(cur); err == nil {
			if link, err := filepath.EvalSymlinks(cur); err == nil {
				cur = link
			}
			break
		}
		tail = append([]string{filepath.Base(cur)}, tail...)
		parent := filepath.Dir(cur)
		if parent == cur {
			break
		}
		cur = parent
	}
	for _, t := range tail {
		cur = filepath.Join(cur, t)
	}
	return cur
}

func isSymlink(p string) bool {
	fi, err := os.Lstat(p)
	return err == nil && fi.Mode()&os.ModeSymlink != 0
}

func assertNotSymlink(p string) error {
	if isSymlink(p) {
		return fmt.Errorf("refuse symlink: %s", p)
	}
	return nil
}

func safeWrite(path string, data []byte, mode os.FileMode) error {
	if err := assertNotSymlink(path); err != nil {
		return err
	}
	parent := filepath.Dir(path)
	if err := os.MkdirAll(parent, 0o700); err != nil {
		return err
	}
	tmp := filepath.Join(parent, fmt.Sprintf(".tmp-%s", hexEncode(randBytesMust(6))))
	f, err := os.OpenFile(tmp, os.O_WRONLY|os.O_CREATE|os.O_EXCL, mode)
	if err != nil {
		return err
	}
	if _, err := f.Write(data); err != nil {
		f.Close()
		os.Remove(tmp)
		return err
	}
	if err := f.Close(); err != nil {
		os.Remove(tmp)
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		os.Remove(tmp)
		return err
	}
	return os.Chmod(path, mode)
}

func chmodBestEffort(path string, mode os.FileMode) {
	_ = os.Chmod(path, mode)
}
