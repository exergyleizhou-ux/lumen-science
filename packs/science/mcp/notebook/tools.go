package notebook

import (
	"context"
	"encoding/json"
	"fmt"
	"os/exec"
	"strings"

	"github.com/lumen-ai/lumen-science/mcp"
)

// Tools returns MCP tool registrations for the notebook kernel.
func Tools(srv *Server) []mcp.RegisteredTool {
	return []mcp.RegisteredTool{
		{
			Tool: mcp.Tool{
				Name:        "notebook_execute",
				Description: "Execute Python code in a persistent notebook kernel; returns stdout, stderr, and any result or error",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"code": map[string]any{"type": "string"},
					},
					"required": []any{"code"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					Code string `json:"code"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, fmt.Errorf("invalid args: %w", err)
				}
				if p.Code == "" {
					return nil, fmt.Errorf("code is required")
				}
				if err := srv.Kernel.Start(); err != nil {
					return nil, fmt.Errorf("kernel start: %w", err)
				}
				result, err := srv.Kernel.Execute(p.Code)
				if err != nil {
					return nil, fmt.Errorf("execute: %w", err)
				}
				return result, nil
			},
		},
		{
			Tool: mcp.Tool{
				Name:        "notebook_restart",
				Description: "Restart the notebook kernel, clearing all state",
				InputSchema: map[string]any{
					"type": "object",
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				if err := srv.Kernel.Restart(); err != nil {
					return nil, fmt.Errorf("restart: %w", err)
				}
				return map[string]string{"status": "restarted"}, nil
			},
		},
		{
			Tool: mcp.Tool{
				Name:        "notebook_state",
				Description: "Get the current kernel state (alive, pid, uptime, python version)",
				InputSchema: map[string]any{
					"type": "object",
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				state, err := srv.Kernel.State()
				if err != nil {
					return nil, fmt.Errorf("state: %w", err)
				}
				return state, nil
			},
		},
		{
			Tool: mcp.Tool{
				Name:        "notebook_shutdown",
				Description: "Shut down the notebook kernel gracefully",
				InputSchema: map[string]any{
					"type": "object",
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				if err := srv.Kernel.Shutdown(); err != nil {
					return nil, fmt.Errorf("shutdown: %w", err)
				}
				return map[string]string{"status": "shutdown"}, nil
			},
		},
		{
			Tool: mcp.Tool{
				Name:        "manage_packages",
				Description: "Install or uninstall Python packages in the kernel via pip",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"action": map[string]any{
							"type": "string",
							"enum": []any{"install", "uninstall"},
						},
						"package": map[string]any{"type": "string"},
					},
					"required": []any{"action", "package"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					Action  string `json:"action"`
					Package string `json:"package"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, fmt.Errorf("invalid args: %w", err)
				}
				if err := srv.Kernel.Start(); err != nil {
					return nil, fmt.Errorf("kernel start: %w", err)
				}
				var output string
				var err error
				switch p.Action {
				case "install":
					output, err = srv.Kernel.InstallPackage(p.Package)
				case "uninstall":
					output, err = srv.Kernel.UninstallPackage(p.Package)
				default:
					return nil, fmt.Errorf("unknown action %q (must be install or uninstall)", p.Action)
				}
				if err != nil {
					return nil, fmt.Errorf("package %s: %w", p.Action, err)
				}
				return map[string]string{"output": output}, nil
			},
		},
		{
			Tool: mcp.Tool{
				Name:        "manage_environments",
				Description: "List conda environments available on the system",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"action": map[string]any{
							"type": "string",
							"enum": []any{"list"},
						},
					},
					"required": []any{"action"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					Action string `json:"action"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, fmt.Errorf("invalid args: %w", err)
				}
				if p.Action != "list" {
					return nil, fmt.Errorf("unknown action %q (only list is supported)", p.Action)
				}
				envs := listCondaEnvironments()
				return map[string]any{"environments": envs}, nil
			},
		},
	}
}

// condaEnvironment describes a single conda environment.
type condaEnvironment struct {
	Name string `json:"name"`
	Path string `json:"path"`
}

// listCondaEnvironments runs "conda env list --json" and returns parsed entries.
// Returns an empty slice if conda is not available or parsing fails.
func listCondaEnvironments() []condaEnvironment {
	out, err := exec.Command("conda", "env", "list", "--json").Output()
	if err != nil {
		return nil
	}
	// conda env list --json outputs a JSON object with an "envs" array of paths.
	var raw struct {
		Envs []string `json:"envs"`
	}
	if err := json.Unmarshal(out, &raw); err != nil {
		return nil
	}
	var envs []condaEnvironment
	for _, envPath := range raw.Envs {
		name := envPath
		if idx := strings.LastIndex(envPath, "/"); idx >= 0 {
			name = envPath[idx+1:]
		}
		envs = append(envs, condaEnvironment{Name: name, Path: envPath})
	}
	return envs
}
