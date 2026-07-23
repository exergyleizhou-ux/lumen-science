package c2d

import (
	"context"
	"encoding/json"

	scimcp "lumen/internal/science/mcp"
	"lumen/internal/science/native"
)

// Config for C2D MCP server.
type Config struct {
	BaseURL string
	Token   string
}

// Tools returns C2D MCP tool registrations.
func Tools(cfg Config) []scimcp.RegisteredTool {
	client := NewClient(cfg.BaseURL, cfg.Token)
	obj := map[string]any{"type": "object"}

	return []scimcp.RegisteredTool{
		{
			Tool: scimcp.Tool{
				Name:        "list_offer_signals",
				Description: "Public C2D compute signals for catalog datasets (anonymous)",
				InputSchema: obj,
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				data, err := client.ListOfferSignals()
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("oasis-c2d", "signals", apiVersion, data), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "list_algorithms",
				Description: "List C2D algorithms for a dataset (requires user OAuth token)",
				InputSchema: map[string]any{
					"type": obj["type"],
					"properties": map[string]any{
						"dataset_id": map[string]any{"type": "string"},
					},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				if err := native.CheckAuth("list_algorithms", cfg.Token); err != nil {
					return nil, err
				}
				var p struct {
					DatasetID string `json:"dataset_id"`
				}
				_ = json.Unmarshal(args, &p)
				data, err := client.ListAlgorithms(p.DatasetID)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("oasis-c2d", "algorithms:"+p.DatasetID, apiVersion, data), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "submit_job",
				Description: "Submit a C2D compute job (requires user OAuth token)",
				InputSchema: map[string]any{
					"type": obj["type"],
					"properties": map[string]any{
						"dataset_id":     map[string]any{"type": "string"},
						"algorithm_id":   map[string]any{"type": "string"},
						"entitlement_id": map[string]any{"type": "string"},
						"params":         map[string]any{"type": "object"},
					},
					"required": []any{"dataset_id", "algorithm_id"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				if err := native.CheckAuth("submit_c2d_job", cfg.Token); err != nil {
					return nil, err
				}
				var p struct {
					DatasetID     string         `json:"dataset_id"`
					AlgorithmID   string         `json:"algorithm_id"`
					EntitlementID string         `json:"entitlement_id"`
					Params        map[string]any `json:"params"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				data, err := client.SubmitJob(p.DatasetID, p.AlgorithmID, p.EntitlementID, p.Params)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("oasis-c2d", "submit:"+p.AlgorithmID, apiVersion, data), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "get_job_status",
				Description: "Get C2D job status (requires user OAuth token)",
				InputSchema: map[string]any{
					"type": obj["type"],
					"properties": map[string]any{
						"job_id": map[string]any{"type": "string"},
					},
					"required": []any{"job_id"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				if err := native.CheckAuth("get_job_status", cfg.Token); err != nil {
					return nil, err
				}
				var p struct {
					JobID string `json:"job_id"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				data, err := client.GetJobStatus(p.JobID)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("oasis-c2d", "job:"+p.JobID, apiVersion, data), nil
			},
		},
	}
}
