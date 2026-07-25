package reviewer

import (
	"context"
	"encoding/json"

	"github.com/lumen-ai/lumen-science/mcp"
)

// Tools returns MCP tool registrations for the Reviewer server.
func Tools(srv *Server) []mcp.RegisteredTool {
	return []mcp.RegisteredTool{
		{
			Tool: mcp.Tool{
				Name:        "start_review",
				Description: "Run an integrity review of all artifacts for a project and run.",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"project_id": map[string]any{"type": "string", "description": "Project identifier"},
						"run_id":     map[string]any{"type": "string", "description": "Run identifier"},
					},
					"required": []any{"project_id", "run_id"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					ProjectID string `json:"project_id"`
					RunID     string `json:"run_id"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				report, err := srv.Reviewer.Review(p.ProjectID, p.RunID)
				if err != nil {
					return mcp.ErrorResult(err.Error()), nil
				}
				return mcp.TextResult(report), nil
			},
		},
		{
			Tool: mcp.Tool{
				Name:        "review_status",
				Description: "Return the latest review report for a project and run (stateless).",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"project_id": map[string]any{"type": "string", "description": "Project identifier"},
						"run_id":     map[string]any{"type": "string", "description": "Run identifier"},
					},
					"required": []any{"project_id", "run_id"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					ProjectID string `json:"project_id"`
					RunID     string `json:"run_id"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				report, err := srv.Reviewer.Review(p.ProjectID, p.RunID)
				if err != nil {
					return mcp.ErrorResult(err.Error()), nil
				}
				return mcp.TextResult(report), nil
			},
		},
		{
			Tool: mcp.Tool{
				Name:        "approve_fix",
				Description: "Re-run the review to confirm that a previously-failing artifact has been corrected.",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"project_id": map[string]any{"type": "string", "description": "Project identifier"},
						"run_id":     map[string]any{"type": "string", "description": "Run identifier"},
					},
					"required": []any{"project_id", "run_id"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					ProjectID string `json:"project_id"`
					RunID     string `json:"run_id"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				report, err := srv.Reviewer.Review(p.ProjectID, p.RunID)
				if err != nil {
					return mcp.ErrorResult(err.Error()), nil
				}
				return mcp.TextResult(report), nil
			},
		},
	}
}
