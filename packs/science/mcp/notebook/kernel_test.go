package notebook

import (
	"os/exec"
	"strings"
	"testing"
)

func skipIfNoPython(t *testing.T) {
	t.Helper()
	if _, err := exec.LookPath("python3"); err != nil {
		t.Skip("python3 not found in PATH")
	}
}

func TestStartStop(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	state, err := k.State()
	if err != nil {
		t.Fatalf("State: %v", err)
	}
	if !state.Alive {
		t.Fatal("expected kernel alive after Start")
	}
	if state.PID == 0 {
		t.Fatal("expected non-zero PID")
	}
	if err := k.Shutdown(); err != nil {
		t.Fatalf("Shutdown: %v", err)
	}
	state, err = k.State()
	if err != nil {
		t.Fatalf("State after shutdown: %v", err)
	}
	if state.Alive {
		t.Fatal("expected kernel not alive after Shutdown")
	}
}

func TestExecute(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	result, err := k.Execute(`print("hello, notebook")`)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if !result.OK {
		t.Fatalf("expected OK=true, got %v", result)
	}
	if !strings.Contains(result.Stdout, "hello, notebook") {
		t.Fatalf("expected stdout to contain 'hello, notebook', got %q", result.Stdout)
	}
}

func TestExecuteError(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	result, err := k.Execute(`raise TypeError("test error")`)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if result.OK {
		t.Fatal("expected OK=false for invalid Python")
	}
	if result.Result == nil {
		t.Fatal("expected result.error for invalid Python")
	}
}

func TestState(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	state, err := k.State()
	if err != nil {
		t.Fatalf("State: %v", err)
	}
	if !state.Alive {
		t.Fatal("expected alive")
	}
	if state.PID == 0 {
		t.Fatal("expected non-zero PID")
	}
	if state.UptimeSeconds <= 0 {
		t.Fatal("expected positive uptime")
	}
	if state.PythonVersion == "" {
		t.Fatal("expected non-empty python version")
	}
}
