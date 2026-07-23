package oasis

import (
	"context"
	"encoding/json"

	scimcp "lumen/internal/science/mcp"
	"lumen/internal/science/native"
)

// Config holds runtime settings for the Oasis MCP server.
type Config struct {
	BaseURL string
	Token   string
}

// Tools returns MCP tool registrations for the 绿洲 marketplace.
func Tools(cfg Config) []scimcp.RegisteredTool {
	client := NewClient(cfg.BaseURL, cfg.Token)
	objectSchema := map[string]any{"type": "object"}

	return []scimcp.RegisteredTool{
		{
			Tool: scimcp.Tool{
				Name:        "search_datasets",
				Description: "Search published datasets on 绿洲 (anonymous metadata)",
				InputSchema: mergeSchema(objectSchema, map[string]any{
					"properties": map[string]any{
						"q":      map[string]any{"type": "string", "description": "keyword search"},
						"limit":  map[string]any{"type": "integer", "default": 20},
						"offset": map[string]any{"type": "integer", "default": 0},
					},
				}),
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					Q      string `json:"q"`
					Limit  int    `json:"limit"`
					Offset int    `json:"offset"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				items, err := client.SearchDatasets(p.Q, p.Limit, p.Offset)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("oasis", p.Q, apiVersion, map[string]any{
					"datasets": items,
					"count":    len(items),
					"base_url": client.BaseURL,
				}), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "list_verified_datasets",
				Description: "List datasets with quality_verified=true (anonymous)",
				InputSchema: mergeSchema(objectSchema, map[string]any{
					"properties": map[string]any{
						"limit": map[string]any{"type": "integer", "default": 10},
					},
				}),
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					Limit int `json:"limit"`
				}
				_ = json.Unmarshal(args, &p)
				items, err := client.ListVerifiedDatasets(p.Limit)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("oasis", "verified_only", apiVersion, map[string]any{
					"datasets": items,
					"count":    len(items),
				}), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "get_dataset_detail",
				Description: "Get one dataset's public metadata by id (anonymous)",
				InputSchema: mergeSchema(objectSchema, map[string]any{
					"properties": map[string]any{
						"dataset_id": map[string]any{"type": "string"},
					},
					"required": []any{"dataset_id"},
				}),
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					DatasetID string `json:"dataset_id"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				d, err := client.GetDatasetDetail(p.DatasetID)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("oasis", p.DatasetID, apiVersion, d), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "preview_schema",
				Description: "Preview sample rows and inferred columns (requires user OAuth token)",
				InputSchema: mergeSchema(objectSchema, map[string]any{
					"properties": map[string]any{
						"dataset_id": map[string]any{"type": "string"},
					},
					"required": []any{"dataset_id"},
				}),
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				if err := CheckAuth("preview_schema", client.Token); err != nil {
					return nil, err
				}
				var p struct {
					DatasetID string `json:"dataset_id"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				row, err := client.PreviewSchema(p.DatasetID)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("oasis", "preview:"+p.DatasetID, apiVersion, row), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "get_verification_certificate",
				Description: "Fetch integrity & registration certificate (requires user OAuth token)",
				InputSchema: mergeSchema(objectSchema, map[string]any{
					"properties": map[string]any{
						"dataset_id": map[string]any{"type": "string"},
					},
					"required": []any{"dataset_id"},
				}),
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				if err := CheckAuth("get_verification_certificate", client.Token); err != nil {
					return nil, err
				}
				var p struct {
					DatasetID string `json:"dataset_id"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				cert, err := client.GetVerificationCertificate(p.DatasetID)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("oasis", "cert:"+p.DatasetID, apiVersion, cert), nil
			},
		},
	}
}

func mergeSchema(base, extra map[string]any) map[string]any {
	out := map[string]any{}
	for k, v := range base {
		out[k] = v
	}
	for k, v := range extra {
		out[k] = v
	}
	return out
}

// CheckAuth returns an error if tool requires user token but client has none.
func CheckAuth(tool string, token string) error {
	if native.RequiredAuth(tool) == native.AuthUserToken && token == "" {
		return &native.AuthError{Tool: tool}
	}
	return nil
}
