// Package platform provides minimal, zero-dependency cross-platform
// primitives for file operations, browser launching, and keychain setup.
// No imports from other Lumen science packages to avoid cycles.
package platform

import (
	"fmt"
	"os"
	"os/exec"
	"runtime"
)

// OpenBrowser opens a URL in the system default browser.
func OpenBrowser(url string) error {
	switch runtime.GOOS {
	case "windows":
		return exec.Command("cmd", "/c", "start", "", url).Run()
	case "darwin":
		return exec.Command("open", url).Run()
	default:
		return exec.Command("xdg-open", url).Run()
	}
}

// OpenDir opens a directory in the system file manager.
func OpenDir(path string) error {
	if _, err := os.Stat(path); err != nil {
		return fmt.Errorf("path not found: %s", path)
	}
	switch runtime.GOOS {
	case "windows":
		return exec.Command("explorer", path).Run()
	case "darwin":
		return exec.Command("open", path).Run()
	default:
		return exec.Command("xdg-open", path).Run()
	}
}

// CopyDir recursively copies a directory.
// On macOS tries APFS clone first, on Windows uses robocopy.
func CopyDir(src, dst string) error {
	if _, err := os.Stat(src); err != nil {
		return fmt.Errorf("source not found: %s", src)
	}
	if _, err := os.Stat(dst); err == nil {
		return nil // already exists
	}

	switch runtime.GOOS {
	case "windows":
		return exec.Command("robocopy", src, dst, "/E",
			"/NFL", "/NDL", "/NJH", "/NJS").Run()
	case "darwin":
		cmd := exec.Command("cp", "-Rc", src, dst)
		if err := cmd.Run(); err == nil {
			return nil
		}
		fallthrough
	default:
		return exec.Command("cp", "-R", src, dst).Run()
	}
}

// EnsureKeychain sets up sandbox keychain on macOS. No-op on other OS.
func EnsureKeychain(sandboxHome string) error {
	if runtime.GOOS != "darwin" {
		return nil
	}
	kc := sandboxHome + "/Library/Keychains/login.keychain-db"
	if _, err := os.Stat(kc); os.IsNotExist(err) {
		_ = os.MkdirAll(kc[:len(kc)-17], 0o700)
		cmd := exec.Command("security", "create-keychain", "-p", "", kc)
		cmd.Env = append(os.Environ(), "HOME="+sandboxHome)
		if err := cmd.Run(); err != nil {
			return fmt.Errorf("keychain create: %w", err)
		}
	}
	for _, args := range [][]string{
		{"list-keychains", "-d", "user", "-s", kc},
		{"default-keychain", "-d", "user", "-s", kc},
		{"unlock-keychain", "-p", "", kc},
		{"set-keychain-settings", kc},
	} {
		cmd := exec.Command("security", args...)
		cmd.Env = append(os.Environ(), "HOME="+sandboxHome)
		_ = cmd.Run()
	}
	return nil
}
