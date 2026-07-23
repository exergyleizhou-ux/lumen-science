// Package langgraph provides an optional LangGraph sidecar API for the science lab.
// It does NOT replace the existing Go agent/chat/SSE pipeline — it runs alongside
// as an independent service callable via POST /api/lab/langgraph/run.
//
// Availability depends on LUMEN_LANGGRAPH being set and the langgraph Python
// package being installed in a venv. When unavailable, the health endpoint
// reports available:false and the run endpoint returns a clear error.
package langgraph

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// IsAvailable checks whether the LangGraph Python environment is ready.
func IsAvailable() bool {
	if os.Getenv("LUMEN_LANGGRAPH") != "1" {
		return false
	}
	// Check for langgraph package in the expected venv
	venvDir := os.Getenv("LUMEN_LANGGRAPH_VENV")
	if venvDir == "" {
		venvDir = filepath.Join(os.Getenv("HOME"), ".lumen/langgraph-venv")
	}
	python := filepath.Join(venvDir, "bin", "python3")
	if st, err := os.Stat(python); err != nil || st.IsDir() {
		// fallback: try system python
		if _, err := exec.LookPath("python3"); err != nil {
			return false
		}
		python = "python3"
	}
	// Quick check: can we import langgraph?
	cmd := exec.Command(python, "-c", "import langgraph")
	cmd.Env = append(os.Environ(), "PYTHONUNBUFFERED=1")
	err := cmd.Run()
	return err == nil
}

// PythonBin returns the python binary to use for LangGraph tasks.
func PythonBin() string {
	venvDir := os.Getenv("LUMEN_LANGGRAPH_VENV")
	if venvDir == "" {
		venvDir = filepath.Join(os.Getenv("HOME"), ".lumen/langgraph-venv")
	}
	candidate := filepath.Join(venvDir, "bin", "python3")
	if st, err := os.Stat(candidate); err == nil && !st.IsDir() {
		return candidate
	}
	if p, err := exec.LookPath("python3"); err == nil {
		return p
	}
	return "python3"
}

// RunRequest is the input to the LangGraph run endpoint.
type RunRequest struct {
	ProjectID string `json:"project_id"`
	Prompt    string `json:"prompt"`
	// Workspace is an absolute path to the project workspace directory.
	// Usually filled by the lab API from project slug; clients may omit it.
	Workspace string `json:"workspace,omitempty"`
}

// RunResponse is the output from the LangGraph run endpoint.
type RunResponse struct {
	OK     bool   `json:"ok"`
	Result string `json:"result,omitempty"`
	Error  string `json:"error,omitempty"`
	Mode   string `json:"mode,omitempty"` // heuristic | llm
}

// Run executes a LangGraph prompt against the Python sidecar.
// Returns a structured response; never panics.
func Run(ctx context.Context, req RunRequest) RunResponse {
	if !IsAvailable() {
		return RunResponse{
			OK:    false,
			Error: "LangGraph 不可用：设置 LUMEN_LANGGRAPH=1 并安装 langgraph (pip install langgraph langchain-core)。详见 docs/lab/LANGGRAPH.md",
		}
	}
	if strings.TrimSpace(req.Prompt) == "" {
		return RunResponse{OK: false, Error: "prompt 不能为空"}
	}

	python := PythonBin()
	// Locate the runner script: same directory as this source, or LUMEN_LANGGRAPH_SCRIPT
	script := os.Getenv("LUMEN_LANGGRAPH_SCRIPT")
	if script == "" {
		// Default: look for langgraph_runner.py next to the lab binary or in ~/.lumen/
		script = filepath.Join(os.Getenv("HOME"), ".lumen", "langgraph_runner.py")
	}

	ctx, cancel := context.WithTimeout(ctx, 120*time.Second)
	defer cancel()

	args := []string{
		script,
		"--project-id", req.ProjectID,
		"--prompt", req.Prompt,
	}
	if ws := strings.TrimSpace(req.Workspace); ws != "" {
		args = append(args, "--workspace", ws)
	}

	var stderr bytes.Buffer
	cmd := exec.CommandContext(ctx, python, args...)
	cmd.Env = append(os.Environ(), "PYTHONUNBUFFERED=1")
	cmd.Stderr = &stderr

	out, err := cmd.Output()
	if err != nil {
		errMsg := stderr.String()
		if errMsg == "" {
			errMsg = err.Error()
		}
		return RunResponse{OK: false, Error: strings.TrimSpace(errMsg)}
	}

	var resp RunResponse
	if err := json.Unmarshal(out, &resp); err != nil {
		// Raw output as result
		return RunResponse{OK: true, Result: strings.TrimSpace(string(out))}
	}
	return resp
}

// LLMReady reports whether an API key is present for optional LLM synthesize.
func LLMReady() bool {
	if os.Getenv("LUMEN_LANGGRAPH_LLM") == "0" {
		return false
	}
	for _, k := range []string{"DEEPSEEK_API_KEY", "OPENAI_API_KEY", "MOONSHOT_API_KEY", "DASHSCOPE_API_KEY"} {
		if strings.TrimSpace(os.Getenv(k)) != "" {
			return true
		}
	}
	return false
}

// Health returns the langgraph section for the lab health endpoint.
func Health() map[string]any {
	available := IsAvailable()
	hint := "设置 LUMEN_LANGGRAPH=1 并安装 langgraph (pip install langgraph langchain-core)"
	if available {
		if LLMReady() {
			hint = "LangGraph 旁路可用（LLM 合成已就绪）"
		} else {
			hint = "LangGraph 旁路可用（启发式；配置科学模型密钥可启用 LLM）"
		}
	}
	return map[string]any{
		"available": available,
		"llm":       available && LLMReady(),
		"hint":      hint,
	}
}
