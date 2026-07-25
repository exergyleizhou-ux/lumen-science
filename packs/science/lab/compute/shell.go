// Package compute — platform-aware shell detection for local job execution.
//
// On Unix: prefers bash, falls back to sh.
// On Windows: prefers bash (Git Bash / MSYS2), then cmd.exe (which supports
// POSIX-like && and || chaining), then powershell as last resort.

package compute

import (
	"os/exec"
	"runtime"
)

// localShell returns the best-available local shell binary and its
// argument prefix for executing a user-supplied command string.
//
// The returned args already include the flag that accepts the command
// as the next argument (e.g. "-c" for bash/sh, "/c" for cmd).
// Callers should append the command string after these args.
func localShell() (shell string, args []string) {
	if runtime.GOOS == "windows" {
		// Prefer a POSIX-compatible shell (Git Bash, MSYS2, Cygwin).
		if _, err := exec.LookPath("bash"); err == nil {
			return "bash", []string{"-lc"}
		}
		if _, err := exec.LookPath("sh"); err == nil {
			return "sh", []string{"-lc"}
		}
		// cmd.exe supports &&, ||, > redirection — the POSIX-like
		// syntax that science job commands use.
		if _, err := exec.LookPath("cmd"); err == nil {
			return "cmd", []string{"/c"}
		}
		// Last resort: PowerShell. May fail with POSIX && chaining.
		return "powershell", []string{"-NoProfile", "-NonInteractive", "-Command"}
	}

	if _, err := exec.LookPath("bash"); err == nil {
		return "bash", []string{"-lc"}
	}
	return "sh", []string{"-lc"}
}

// localShellEcho returns a localShell invocation that prints "ok".
// Used by the host ping endpoint to verify shell availability.
func localShellEcho() (shell string, args []string) {
	shell, args = localShell()
	return shell, append(args, "echo ok")
}
