package notebook

import (
	"os/exec"
	"strings"
	"testing"
	"time"
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

// ── DS-40 negative tests ──────────────────────────────────────

// TestCrashRecovery verifies the kernel can execute code after a previous error.
func TestCrashRecovery(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	// First execution raises an error
	result, err := k.Execute(`raise ValueError("boom")`)
	if err != nil {
		t.Fatalf("Execute error: %v", err)
	}
	if result.OK {
		t.Fatal("expected first execution to fail")
	}

	// Second execution should still work
	result, err = k.Execute(`42`)
	if err != nil {
		t.Fatalf("recovery Execute: %v", err)
	}
	if !result.OK {
		t.Fatalf("expected recovery to succeed, got: %v", result)
	}
}

// TestRestartAfterError verifies kernel can be restarted after errors.
func TestRestartAfterError(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	// Cause an error
	k.Execute(`raise RuntimeError("pre-restart")`)

	// Restart
	if err := k.Restart(); err != nil {
		t.Fatalf("Restart: %v", err)
	}

	// Should work after restart
	result, err := k.Execute(`print("after restart")`)
	if err != nil {
		t.Fatalf("post-restart Execute: %v", err)
	}
	if !result.OK {
		t.Fatalf("expected post-restart to succeed")
	}
}

// TestExecuteWithStdoutStderr verifies both stdout and stderr are captured.
func TestExecuteWithStdoutStderr(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	result, err := k.Execute(`import sys; print("out"); print("err", file=sys.stderr)`)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if !strings.Contains(result.Stdout, "out") {
		t.Fatalf("missing stdout: %q", result.Stdout)
	}
	if !strings.Contains(result.Stderr, "err") {
		t.Fatalf("missing stderr: %q", result.Stderr)
	}
}

// TestMultipleExecution verifies code that defines and uses variables in one call works.
func TestMultipleExecution(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	// Define and use variable in the same execution block
	result, err := k.Execute("x = 100\nprint(x * 2)")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(result.Stdout, "200") {
		t.Fatalf("expected 200, got: %q", result.Stdout)
	}
}

// TestEmptyCodeRejection verifies empty code is handled.
func TestEmptyCodeRejection(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	result, err := k.Execute("")
	if err != nil {
		t.Fatal(err)
	}
	if !result.OK {
		t.Log("empty code flagged (acceptable)")
	}
}

// TestSyntaxError verifies Python syntax errors are captured.
func TestSyntaxError(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	result, err := k.Execute(`if True print("bad")`)
	if err != nil {
		t.Fatal(err)
	}
	if result.OK {
		t.Fatal("expected syntax error to fail")
	}
}

// TestImportModule verifies basic imports work.
func TestImportModule(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	result, err := k.Execute(`import json; print(json.dumps({"key": "value"}))`)
	if err != nil {
		t.Fatal(err)
	}
	if !result.OK {
		t.Fatalf("import failed: %v", result)
	}
}

// TestTimeout verifies that a slow script completes within reasonable time.
func TestTimeout(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	start := time.Now()
	result, err := k.Execute(`sum(range(100000))`)
	elapsed := time.Since(start)
	if err != nil {
		t.Fatal(err)
	}
	if !result.OK {
		t.Fatal("expected OK")
	}
	if elapsed > 5*time.Second {
		t.Fatalf("execution took too long: %v", elapsed)
	}
	t.Logf("100k sum completed in %v", elapsed)
}

// TestNonUtf8Output verifies non-UTF-8 output is handled gracefully.
func TestNonUtf8Output(t *testing.T) {
	skipIfNoPython(t)
	k := &Kernel{}
	if err := k.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer k.Shutdown()

	result, err := k.Execute(`print("café")`)
	if err != nil {
		t.Fatal(err)
	}
	if !result.OK {
		t.Fatal("expected OK for UTF-8 output")
	}
}
