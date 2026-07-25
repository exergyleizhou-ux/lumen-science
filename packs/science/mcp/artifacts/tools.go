package artifacts

import (
	"context"
	"encoding/base64"
	"encoding/json"

	"github.com/lumen-ai/lumen-science/mcp"
)

// Tools returns MCP tool registrations for the Artifacts server.
func Tools(store *Store) []mcp.RegisteredTool {
	return []mcp.RegisteredTool{
		{
			Tool: mcp.Tool{
				Name:        "artifact_write",
				Description: "Register a file as a durable artifact. Content must be base64-encoded.",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"project_id": map[string]any{"type": "string", "description": "Project identifier"},
						"run_id":     map[string]any{"type": "string", "description": "Run identifier"},
						"path":       map[string]any{"type": "string", "description": "Relative artifact path"},
						"content":    map[string]any{"type": "string", "description": "Base64-encoded file content"},
						"mime_type":  map[string]any{"type": "string", "description": "MIME type"},
						"label":      map[string]any{"type": "string", "description": "Human-readable label"},
					},
					"required": []any{"project_id", "run_id", "path", "content", "mime_type"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					ProjectID string `json:"project_id"`
					RunID     string `json:"run_id"`
					Path      string `json:"path"`
					Content   string `json:"content"`
					MimeType  string `json:"mime_type"`
					Label     string `json:"label"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				data, err := base64.StdEncoding.DecodeString(p.Content)
				if err != nil {
					return mcp.ErrorResult("invalid base64 content"), nil
				}
				meta, err := store.Write(p.ProjectID, p.RunID, p.Path, p.Label, p.MimeType, data)
				if err != nil {
					return mcp.ErrorResult(err.Error()), nil
				}
				return mcp.TextResult(meta), nil
			},
		},
		{
			Tool: mcp.Tool{
				Name:        "artifact_list",
				Description: "List artifact metadata for a project and run.",
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
				metas, err := store.List(p.ProjectID, p.RunID)
				if err != nil {
					return mcp.ErrorResult(err.Error()), nil
				}
				if metas == nil {
					metas = []ArtifactMeta{}
				}
				return mcp.TextResult(metas), nil
			},
		},
		{
			Tool: mcp.Tool{
				Name:        "artifact_read",
				Description: "Read an artifact's content (base64-encoded) and metadata.",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"project_id": map[string]any{"type": "string", "description": "Project identifier"},
						"run_id":     map[string]any{"type": "string", "description": "Run identifier"},
						"path":       map[string]any{"type": "string", "description": "Relative artifact path"},
					},
					"required": []any{"project_id", "run_id", "path"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					ProjectID string `json:"project_id"`
					RunID     string `json:"run_id"`
					Path      string `json:"path"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				data, meta, err := store.Read(p.ProjectID, p.RunID, p.Path)
				if err != nil {
					return mcp.ErrorResult(err.Error()), nil
				}
				result := map[string]any{
					"content": base64.StdEncoding.EncodeToString(data),
				}
				if meta != nil {
					result["meta"] = meta
				}
				return mcp.TextResult(result), nil
			},
		},
		{
			Tool: mcp.Tool{
				Name:        "artifact_preview",
				Description: "Return a content-aware preview of an artifact (CSV columns/rows, FASTA seq count, etc.).",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"project_id": map[string]any{"type": "string", "description": "Project identifier"},
						"run_id":     map[string]any{"type": "string", "description": "Run identifier"},
						"path":       map[string]any{"type": "string", "description": "Relative artifact path"},
					},
					"required": []any{"project_id", "run_id", "path"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					ProjectID string `json:"project_id"`
					RunID     string `json:"run_id"`
					Path      string `json:"path"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				preview, err := store.Preview(p.ProjectID, p.RunID, p.Path)
				if err != nil {
					return mcp.ErrorResult(err.Error()), nil
				}
				return mcp.TextResult(preview), nil
			},
		},
	}
}
