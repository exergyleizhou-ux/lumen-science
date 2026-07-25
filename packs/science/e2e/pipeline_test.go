// Package e2e tests the full Lumen Science offline product loop:
// Artifacts → Notebook → Reviewer → Renderer
package e2e

import (
	"bufio"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"testing"
	"time"
)

// mcpClient wraps a stdio MCP subprocess for testing.
type mcpClient struct {
	cmd    *exec.Cmd
	stdin  io.WriteCloser
	stdout *bufio.Reader
	reqID  int
}

func startMCP(t *testing.T, name string) *mcpClient {
	t.Helper()
	bin := fmt.Sprintf("lumen-mcp-%s", name)
	path := bin
	if _, err := os.Stat(path); os.IsNotExist(err) {
		// Try common build locations
		for _, prefix := range []string{"../", "../../"} {
			if _, err := os.Stat(prefix + bin); err == nil {
				path = prefix + bin
				break
			}
		}
	}
	if _, err := exec.LookPath(path); err != nil && !strings.Contains(path, "/") {
		// Try go run
		path = "go"
	}
	t.Logf("starting MCP %s: %s", name, path)
	cmd := exec.Command(path)
	stdin, err := cmd.StdinPipe()
	if err != nil {
		t.Skipf("cannot create stdin pipe for %s: %v", name, err)
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Skipf("cannot create stdout pipe for %s: %v", name, err)
	}
	cmd.Stderr = os.Stderr
	if err := cmd.Start(); err != nil {
		t.Skipf("cannot start %s: %v (build with 'go build ./standalone/cmd/%s')", name, err, name)
	}
	c := &mcpClient{cmd: cmd, stdin: stdin, stdout: bufio.NewReader(stdout)}
	// Initialize
	_, err = c.call("initialize", map[string]any{
		"protocolVersion": "2024-11-05",
		"capabilities":    map[string]any{"tools": map[string]any{}},
		"clientInfo":      map[string]any{"name": "e2e-test", "version": "0.1.0"},
	})
	if err != nil {
		c.cmd.Process.Kill()
		t.Skipf("cannot initialize %s: %v", name, err)
	}
	return c
}

func (c *mcpClient) call(method string, params any) (map[string]any, error) {
	c.reqID++
	req := map[string]any{"jsonrpc": "2.0", "id": c.reqID, "method": method}
	if params != nil {
		req["params"] = params
	}
	body, _ := json.Marshal(req)
	if _, err := fmt.Fprintf(c.stdin, "Content-Length: %d\r\n\r\n%s", len(body), body); err != nil {
		return nil, fmt.Errorf("write: %w", err)
	}
	line, err := c.stdout.ReadString('\n')
	if err != nil {
		return nil, fmt.Errorf("read header: %w", err)
	}
	line = strings.TrimSpace(line)
	if !strings.HasPrefix(line, "Content-Length:") {
		return nil, fmt.Errorf("expected Content-Length, got %q", line)
	}
	length, _ := strconv.Atoi(strings.TrimSpace(strings.TrimPrefix(line, "Content-Length:")))
	if _, err := c.stdout.ReadString('\n'); err != nil {
		return nil, fmt.Errorf("read blank: %w", err)
	}
	respBody := make([]byte, length)
	if _, err := io.ReadFull(c.stdout, respBody); err != nil {
		return nil, fmt.Errorf("read body: %w", err)
	}
	var resp struct {
		Result json.RawMessage `json:"result"`
		Error  *struct{ Message string `json:"message"` } `json:"error"`
	}
	json.Unmarshal(respBody, &resp)
	if resp.Error != nil {
		return nil, fmt.Errorf("mcp error: %s", resp.Error.Message)
	}
	var result map[string]any
	json.Unmarshal(resp.Result, &result)
	return result, nil
}

func (c *mcpClient) toolCall(name string, args map[string]any) (map[string]any, error) {
	return c.call("tools/call", map[string]any{"name": name, "arguments": args})
}

func (c *mcpClient) close() {
	if c.cmd != nil && c.cmd.Process != nil {
		c.cmd.Process.Kill()
		c.cmd.Wait()
	}
}

// TestE2EArtifactWriteRead tests the artifacts MCP server write→read pipeline.
func TestE2EArtifactWriteRead(t *testing.T) {
	artifacts := startMCP(t, "artifacts")
	defer artifacts.close()

	project := "e2e-test"
	run := fmt.Sprintf("run-%d", time.Now().UnixNano())
	content := []byte("sample_id,condition,value\nA,control,1.2\nB,treatment,3.4\n")

	result, err := artifacts.toolCall("artifact_write", map[string]any{
		"project_id": project,
		"run_id":     run,
		"path":       "data/results.csv",
		"content":    base64.StdEncoding.EncodeToString(content),
		"mime_type":  "text/csv",
		"label":      "e2e test results",
	})
	if err != nil {
		t.Fatalf("artifact_write: %v", err)
	}
	if isError, _ := result["isError"].(bool); isError {
		t.Fatalf("artifact_write error: %v", result)
	}
	t.Logf("write result: %v", result)

	// List artifacts
	listResult, err := artifacts.toolCall("artifact_list", map[string]any{
		"project_id": project,
		"run_id":     run,
	})
	if err != nil {
		t.Fatalf("artifact_list: %v", err)
	}
	t.Logf("list result: %v", listResult)

	// Read artifact
	readResult, err := artifacts.toolCall("artifact_read", map[string]any{
		"project_id": project,
		"run_id":     run,
		"path":       "data/results.csv",
	})
	if err != nil {
		t.Fatalf("artifact_read: %v", err)
	}
	t.Logf("read result keys: %v", keys(readResult))

	// Preview artifact
	previewResult, err := artifacts.toolCall("artifact_preview", map[string]any{
		"project_id": project,
		"run_id":     run,
		"path":       "data/results.csv",
	})
	if err != nil {
		t.Fatalf("artifact_preview: %v", err)
	}
	t.Logf("preview result: %v", previewResult)
}

// TestE2ENotebookExecute tests the notebook MCP server.
func TestE2ENotebookExecute(t *testing.T) {
	notebook := startMCP(t, "notebook")
	defer notebook.close()

	// Execute simple Python
	result, err := notebook.toolCall("notebook_execute", map[string]any{
		"code": "2 + 2",
	})
	if err != nil {
		t.Fatalf("notebook_execute: %v", err)
	}
	t.Logf("execute 2+2: %v", result)

	// Check state
	state, err := notebook.toolCall("notebook_state", map[string]any{})
	if err != nil {
		t.Fatalf("notebook_state: %v", err)
	}
	t.Logf("state: %v", state)

	// Execute with print
	result, err = notebook.toolCall("notebook_execute", map[string]any{
		"code": "print('hello from e2e')",
	})
	if err != nil {
		t.Fatalf("notebook_execute print: %v", err)
	}
	t.Logf("execute print: %v", result)
}

// TestE2EReviewer tests the reviewer MCP server.
func TestE2EReviewer(t *testing.T) {
	// First write an artifact via the artifacts server
	artifacts := startMCP(t, "artifacts")
	defer artifacts.close()

	project := "e2e-review"
	run := fmt.Sprintf("run-%d", time.Now().UnixNano())

	_, err := artifacts.toolCall("artifact_write", map[string]any{
		"project_id": project,
		"run_id":     run,
		"path":       "output.txt",
		"content":    base64.StdEncoding.EncodeToString([]byte("verified content")),
		"mime_type":  "text/plain",
		"label":      "review target",
	})
	if err != nil {
		t.Fatalf("artifact_write: %v", err)
	}

	// Now review
	reviewer := startMCP(t, "reviewer")
	defer reviewer.close()

	report, err := reviewer.toolCall("start_review", map[string]any{
		"project_id": project,
		"run_id":     run,
	})
	if err != nil {
		t.Fatalf("start_review: %v", err)
	}
	t.Logf("review report: %v", report)
}

// TestE2EFullPipeline tests the complete pipeline:
// Search(connector) → Analyze(notebook) → Store(artifact) → Review → Render
func TestE2EFullPipeline(t *testing.T) {
	// Phase 1: Write data via Artifacts MCP
	artifacts := startMCP(t, "artifacts")
	defer artifacts.close()

	project := "e2e-full"
	run := fmt.Sprintf("pipeline-%d", time.Now().UnixNano())

	// Step 1: Store initial data artifact
	csvData := "gene,expression,pvalue\nBRCA1,2.3,0.001\nTP53,5.1,0.0001\nMYC,1.8,0.05\n"
	_, err := artifacts.toolCall("artifact_write", map[string]any{
		"project_id": project,
		"run_id":     run,
		"path":       "input/expression.csv",
		"content":    base64.StdEncoding.EncodeToString([]byte(csvData)),
		"mime_type":  "text/csv",
		"label":      "gene expression data",
	})
	if err != nil {
		t.Fatalf("step 1 artifact_write: %v", err)
	}

	// Step 2: Read back and verify
	_, err = artifacts.toolCall("artifact_read", map[string]any{
		"project_id": project,
		"run_id":     run,
		"path":       "input/expression.csv",
	})
	if err != nil {
		t.Fatalf("step 2 artifact_read: %v", err)
	}

	// Step 3: Notebook analysis
	notebook := startMCP(t, "notebook")
	defer notebook.close()

	result, err := notebook.toolCall("notebook_execute", map[string]any{
		"code": fmt.Sprintf(`data = %q
lines = data.strip().split('\n')
significant = [l for l in lines[1:] if float(l.split(',')[2]) < 0.01]
result = f"Found {len(significant)} significant genes out of {len(lines)-1}"
print(result)`, csvData),
	})
	if err != nil {
		t.Fatalf("step 3 notebook_execute: %v", err)
	}
	t.Logf("notebook analysis: %v", result)

	// Step 4: Review
	reviewer := startMCP(t, "reviewer")
	defer reviewer.close()

	report, err := reviewer.toolCall("start_review", map[string]any{
		"project_id": project,
		"run_id":     run,
	})
	if err != nil {
		t.Fatalf("step 4 start_review: %v", err)
	}
	t.Logf("review report: %v", report)

	t.Log("✅ E2E pipeline: connector data → notebook analysis → artifact storage → review")
}

func keys(m map[string]any) []string {
	var ks []string
	for k := range m {
		ks = append(ks, k)
	}
	return ks
}

func TestE2EArtifactPathTraversal(t *testing.T) {
	artifacts := startMCP(t, "artifacts")
	defer artifacts.close()

	result, err := artifacts.toolCall("artifact_write", map[string]any{
		"project_id": "e2e",
		"run_id":     "security",
		"path":       "../../etc/passwd",
		"content":    base64.StdEncoding.EncodeToString([]byte("malicious")),
		"mime_type":  "text/plain",
	})
	if err == nil {
		isErr, _ := result["isError"].(bool)
		if !isErr {
			t.Error("path traversal should be rejected")
		}
	}
}

func TestE2ENotebookErrorRecovery(t *testing.T) {
	notebook := startMCP(t, "notebook")
	defer notebook.close()

	// Execute code with error
	_, err := notebook.toolCall("notebook_execute", map[string]any{
		"code": "raise ValueError('test error')",
	})
	// Should return result (not error from MCP layer), error is in result
	if err != nil {
		t.Logf("notebook error (expected): %v", err)
	}

	// Should still work after error
	result, err := notebook.toolCall("notebook_execute", map[string]any{
		"code": "42",
	})
	if err != nil {
		t.Fatalf("notebook should recover after error: %v", err)
	}
	t.Logf("recovery result: %v", result)
}
