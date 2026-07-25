package notebook

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os/exec"
	"sync"
	"time"
)

const kernelScript = `import sys, json, traceback
def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            rid = req.get("id", 0)
            method = req.get("method", "")
            params = req.get("params", {})
            if method == "execute":
                code = params.get("code", "")
                import io as _io
                _out, _err = _io.StringIO(), _io.StringIO()
                _old_out, _old_err = sys.stdout, sys.stderr
                sys.stdout, sys.stderr = _out, _err
                exec_ok = True
                exec_result = None
                try:
                    exec(code, {"__builtins__": __builtins__})
                except Exception as e:
                    exec_result = {"error": str(e), "traceback": traceback.format_exc()}
                    exec_ok = False
                finally:
                    sys.stdout, sys.stderr = _old_out, _old_err
                resp = {"id": rid, "result": {"stdout": _out.getvalue(), "stderr": _err.getvalue(), "ok": exec_ok, "result": exec_result}}
            elif method == "state":
                resp = {"id": rid, "result": {"python_version": sys.version.split()[0], "platform": sys.platform}}
            elif method == "shutdown":
                resp = {"id": rid, "result": {"status": "shutting_down"}}
                print(json.dumps(resp), flush=True)
                sys.exit(0)
            else:
                resp = {"id": rid, "error": "unknown method: " + method}
            print(json.dumps(resp), flush=True)
        except Exception as e:
            print(json.dumps({"id": 0, "error": str(e)}), flush=True)
if __name__ == "__main__":
    main()
`

// KernelState describes the current status of the Python kernel.
type KernelState struct {
	Alive         bool    `json:"alive"`
	PID           int     `json:"pid,omitempty"`
	UptimeSeconds float64 `json:"uptime_seconds,omitempty"`
	PythonVersion string  `json:"python_version,omitempty"`
}

// ExecuteResult holds the output of a single code execution.
type ExecuteResult struct {
	Stdout string `json:"stdout"`
	Stderr string `json:"stderr"`
	OK     bool   `json:"ok"`
	Result any    `json:"result,omitempty"`
	Error  string `json:"error,omitempty"`
}

// Kernel manages a long-running Python subprocess for code execution.
type Kernel struct {
	cmd       *exec.Cmd
	stdin     io.WriteCloser
	stdout    *bufio.Reader
	mu        sync.Mutex
	alive     bool
	startTime time.Time
	pythonVer string
	reqID     int
}

// Start launches the Python kernel subprocess.
func (k *Kernel) Start() error {
	k.mu.Lock()
	defer k.mu.Unlock()
	if k.alive {
		return nil
	}
	pythonPath := "python3"
	cmd := exec.Command(pythonPath, "-c", kernelScript)
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return fmt.Errorf("stdin pipe: %w", err)
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return fmt.Errorf("stdout pipe: %w", err)
	}
	cmd.Stderr = cmd.Stdout // merge stderr into stdout
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("start python: %w", err)
	}
	k.cmd = cmd
	k.stdin = stdin
	k.stdout = bufio.NewReader(stdout)
	k.alive = true
	k.startTime = time.Now()
	// Get python version from the kernel itself.
	state, err := k.state()
	if err == nil {
		k.pythonVer = state.PythonVersion
	}
	return nil
}

// Execute runs a snippet of Python code in the kernel.
func (k *Kernel) Execute(code string) (*ExecuteResult, error) {
	k.mu.Lock()
	defer k.mu.Unlock()
	if !k.alive {
		return nil, fmt.Errorf("kernel not running")
	}
	return k.call("execute", map[string]string{"code": code})
}

// State returns the current kernel status.
func (k *Kernel) State() (*KernelState, error) {
	k.mu.Lock()
	defer k.mu.Unlock()
	if !k.alive {
		return &KernelState{Alive: false}, nil
	}
	return k.state()
}

// state returns the kernel state without acquiring the mutex.
func (k *Kernel) state() (*KernelState, error) {
	raw, err := k.callRaw("state", nil)
	if err != nil {
		return &KernelState{Alive: false}, err
	}
	ver, _ := raw["python_version"].(string)
	return &KernelState{
		Alive:         true,
		PID:           k.cmd.Process.Pid,
		UptimeSeconds: time.Since(k.startTime).Seconds(),
		PythonVersion: ver,
	}, nil
}

// Restart stops and restarts the kernel.
func (k *Kernel) Restart() error {
	k.Shutdown()
	return k.Start()
}

// Shutdown gracefully terminates the kernel.
func (k *Kernel) Shutdown() error {
	k.mu.Lock()
	defer k.mu.Unlock()
	if !k.alive {
		return nil
	}
	k.alive = false
	// Try graceful shutdown.
	k.call("shutdown", nil)
	// Force kill after 3s.
	done := make(chan error, 1)
	go func() { done <- k.cmd.Wait() }()
	select {
	case <-done:
	case <-time.After(3 * time.Second):
		k.cmd.Process.Kill()
	}
	return nil
}

// InstallPackage installs a Python package via pip inside the kernel process.
func (k *Kernel) InstallPackage(name string) (string, error) {
	code := fmt.Sprintf(
		"import subprocess, sys; r = subprocess.run([sys.executable, '-m', 'pip', 'install', %q], capture_output=True, text=True); print(r.stdout); print(r.stderr, file=sys.stderr)",
		name,
	)
	result, err := k.Execute(code)
	if err != nil {
		return "", err
	}
	return result.Stdout + result.Stderr, nil
}

// UninstallPackage removes a Python package via pip.
func (k *Kernel) UninstallPackage(name string) (string, error) {
	code := fmt.Sprintf(
		"import subprocess, sys; r = subprocess.run([sys.executable, '-m', 'pip', 'uninstall', '-y', %q], capture_output=True, text=True); print(r.stdout); print(r.stderr, file=sys.stderr)",
		name,
	)
	result, err := k.Execute(code)
	if err != nil {
		return "", err
	}
	return result.Stdout + result.Stderr, nil
}

// call sends a JSON-RPC message to the kernel and reads the response.
// Must be called with k.mu held.
func (k *Kernel) call(method string, params any) (*ExecuteResult, error) {
	raw, err := k.callRaw(method, params)
	if err != nil {
		return nil, err
	}
	var result ExecuteResult
	data, _ := json.Marshal(raw)
	json.Unmarshal(data, &result)
	return &result, nil
}

// callRaw sends a JSON-RPC message and returns the raw result map.
// Must be called with k.mu held.
func (k *Kernel) callRaw(method string, params any) (map[string]any, error) {
	k.reqID++
	req := map[string]any{"id": k.reqID, "method": method}
	if params != nil {
		req["params"] = params
	}
	data, _ := json.Marshal(req)
	if _, err := fmt.Fprintf(k.stdin, "%s\n", data); err != nil {
		k.alive = false
		return nil, fmt.Errorf("write to kernel: %w", err)
	}
	line, err := k.stdout.ReadString('\n')
	if err != nil {
		k.alive = false
		return nil, fmt.Errorf("read from kernel: %w", err)
	}
	var resp struct {
		ID     int            `json:"id"`
		Result map[string]any `json:"result,omitempty"`
		Error  string         `json:"error,omitempty"`
	}
	if err := json.Unmarshal([]byte(line), &resp); err != nil {
		return nil, fmt.Errorf("parse response: %w", err)
	}
	if resp.Error != "" {
		return nil, fmt.Errorf("kernel error: %s", resp.Error)
	}
	return resp.Result, nil
}
