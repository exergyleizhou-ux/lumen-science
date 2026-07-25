package compute

import (
	"runtime"
	"strings"
	"testing"
)

func TestLocalShellReturnsValidCommand(t *testing.T) {
	shell, args := localShell()

	if shell == "" {
		t.Fatal("localShell() returned empty shell name")
	}

	if len(args) == 0 {
		t.Fatal("localShell() returned empty args")
	}

	// Verify the shell is a known/expected binary
	switch runtime.GOOS {
	case "windows":
		validWindows := map[string]bool{
			"bash": true, "sh": true, "cmd": true, "powershell": true,
		}
		if !validWindows[shell] {
			t.Errorf("unexpected Windows shell: %s", shell)
		}
	case "darwin", "linux":
		validUnix := map[string]bool{"bash": true, "sh": true}
		if !validUnix[shell] {
			t.Errorf("unexpected Unix shell: %s", shell)
		}
	}

	// Verify the first arg is a flag (starts with - or /)
	firstArg := args[0]
	if !strings.HasPrefix(firstArg, "-") && !strings.HasPrefix(firstArg, "/") {
		t.Errorf("first arg should be a flag, got: %s", firstArg)
	}
}

func TestLocalShellEchoReturnsOk(t *testing.T) {
	shell, args := localShellEcho()

	if shell == "" {
		t.Fatal("localShellEcho() returned empty shell name")
	}

	// The last argument should be "echo ok"
	if len(args) < 2 {
		t.Fatalf("expected at least 2 args, got %d: %v", len(args), args)
	}
	lastArg := args[len(args)-1]
	if !strings.Contains(lastArg, "echo ok") {
		t.Errorf("last arg should contain 'echo ok', got: %s", lastArg)
	}
}

func TestLocalShellConsistency(t *testing.T) {
	// Calling twice should return the same shell (best-available is stable)
	shell1, _ := localShell()
	shell2, _ := localShell()

	if shell1 != shell2 {
		t.Errorf("localShell() is not stable: %s vs %s", shell1, shell2)
	}
}

// Verify localShell and localShellEcho use the same shell binary.
func TestLocalShellEchoUsesSameShell(t *testing.T) {
	s, _ := localShell()
	e, _ := localShellEcho()
	if s != e {
		t.Errorf("localShellEcho uses different shell: %s vs %s", s, e)
	}
}
