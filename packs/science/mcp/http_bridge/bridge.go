// Package http_bridge exposes a stdio MCP server as an HTTP endpoint with Bearer auth.
package http_bridge

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"sync"
)

// Bridge manages a stdio MCP subprocess and exposes it over HTTP.
type Bridge struct {
	command string
	args    []string
	token   string

	mu     sync.Mutex
	cmd    *exec.Cmd
	stdin  io.WriteCloser
	stdout *bufio.Reader
	reqID  int
}

// NewBridge creates a Bridge that launches the given MCP command on first use.
func NewBridge(command string, args []string, token string) *Bridge {
	return &Bridge{command: command, args: args, token: token}
}

// Start launches the underlying MCP subprocess and sends initialize.
func (b *Bridge) Start() error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.cmd != nil {
		return nil // already started
	}
	cmd := exec.Command(b.command, b.args...)
	// Keep the target's stderr visible for debugging.
	cmd.Stderr = os.Stderr

	stdin, err := cmd.StdinPipe()
	if err != nil {
		return fmt.Errorf("stdin pipe: %w", err)
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return fmt.Errorf("stdout pipe: %w", err)
	}
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("start %s: %w", b.command, err)
	}
	b.cmd = cmd
	b.stdin = stdin
	b.stdout = bufio.NewReader(stdout)

	// MCP initialize handshake
	_, err = b.callLocked("initialize", map[string]any{
		"protocolVersion": "2024-11-05",
		"capabilities":    map[string]any{"tools": map[string]any{}},
		"clientInfo":      map[string]any{"name": "lumen-http-bridge", "version": "0.1.0"},
	})
	return err
}

// Shutdown terminates the underlying MCP subprocess.
func (b *Bridge) Shutdown() {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.cmd == nil || b.cmd.Process == nil {
		return
	}
	b.cmd.Process.Kill()
	b.cmd.Wait()
	b.cmd = nil
}

// ServeHTTP handles incoming HTTP MCP tool calls.
func (b *Bridge) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	// Auth
	auth := r.Header.Get("Authorization")
	if !strings.HasPrefix(auth, "Bearer ") || strings.TrimPrefix(auth, "Bearer ") != b.token {
		http.Error(w, `{"error":"unauthorized"}`, http.StatusUnauthorized)
		return
	}

	path := strings.TrimPrefix(r.URL.Path, "/")
	if r.Method != http.MethodPost {
		http.Error(w, `{"error":"method not allowed"}`, http.StatusMethodNotAllowed)
		return
	}

	body, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, `{"error":"bad request"}`, http.StatusBadRequest)
		return
	}
	defer r.Body.Close()

	var req struct {
		Method string          `json:"method"`
		Params json.RawMessage `json:"params,omitempty"`
	}
	if err := json.Unmarshal(body, &req); err != nil {
		http.Error(w, `{"error":"invalid json"}`, http.StatusBadRequest)
		return
	}

	var result any
	switch path {
	case "tools/call":
		result, err = b.CallTool(req.Method, req.Params)
	case "tools/list":
		result, err = b.call("tools/list", nil)
	case "initialize":
		result, err = b.call("initialize", nil)
	default:
		http.Error(w, fmt.Sprintf(`{"error":"unknown endpoint: %s"}`, path), http.StatusNotFound)
		return
	}

	if err != nil {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"error": err.Error()})
		return
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(result)
}

// CallTool invokes a named tool on the underlying MCP server.
func (b *Bridge) CallTool(name string, args json.RawMessage) (any, error) {
	return b.call("tools/call", map[string]any{
		"name":      name,
		"arguments": args,
	})
}

// HealthHandler returns bridge health status.
func (b *Bridge) HealthHandler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	alive := b.cmd != nil && b.cmd.Process != nil
	json.NewEncoder(w).Encode(map[string]any{
		"status":  "ok",
		"target":  b.command,
		"running": alive,
	})
}

// call sends a JSON-RPC request following the MCP transport protocol.
func (b *Bridge) call(method string, params any) (map[string]any, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.callLocked(method, params)
}

func (b *Bridge) callLocked(method string, params any) (map[string]any, error) {
	b.reqID++
	req := map[string]any{
		"jsonrpc": "2.0",
		"id":      b.reqID,
		"method":  method,
	}
	if params != nil {
		req["params"] = params
	}
	body, err := json.Marshal(req)
	if err != nil {
		return nil, fmt.Errorf("marshal request: %w", err)
	}

	// Write MCP transport header + body
	header := fmt.Sprintf("Content-Length: %d\r\n\r\n", len(body))
	if _, err := io.WriteString(b.stdin, header); err != nil {
		b.cmd = nil
		return nil, fmt.Errorf("write header: %w", err)
	}
	if _, err := b.stdin.Write(body); err != nil {
		b.cmd = nil
		return nil, fmt.Errorf("write body: %w", err)
	}

	// Read Content-Length header line
	cl, err := b.stdout.ReadString('\n')
	if err != nil {
		b.cmd = nil
		return nil, fmt.Errorf("read header: %w", err)
	}
	cl = strings.TrimSpace(cl)
	if !strings.HasPrefix(cl, "Content-Length:") {
		return nil, fmt.Errorf("expected Content-Length header, got %q", cl)
	}
	length, err := strconv.Atoi(strings.TrimSpace(strings.TrimPrefix(cl, "Content-Length:")))
	if err != nil {
		return nil, fmt.Errorf("parse Content-Length: %w", err)
	}
	// Skip blank line after header
	if _, err := b.stdout.ReadString('\n'); err != nil {
		return nil, fmt.Errorf("read blank line: %w", err)
	}
	// Read exact body
	respBody := make([]byte, length)
	if _, err := io.ReadFull(b.stdout, respBody); err != nil {
		b.cmd = nil
		return nil, fmt.Errorf("read body: %w", err)
	}

	var resp struct {
		Result json.RawMessage `json:"result,omitempty"`
		Error  *struct {
			Code    int    `json:"code"`
			Message string `json:"message"`
		} `json:"error,omitempty"`
	}
	if err := json.Unmarshal(respBody, &resp); err != nil {
		return nil, fmt.Errorf("parse response: %w", err)
	}
	if resp.Error != nil {
		return nil, fmt.Errorf("mcp error %d: %s", resp.Error.Code, resp.Error.Message)
	}

	var result map[string]any
	if err := json.Unmarshal(resp.Result, &result); err != nil {
		// Result may be a JSON array or primitive; wrap it.
		var raw any
		json.Unmarshal(resp.Result, &raw)
		return map[string]any{"result": raw}, nil
	}
	return result, nil
}
