package geo

import (
	"context"
	"encoding/json"

	scimcp "lumen/internal/science/mcp"
)

// Tools returns MCP registrations for GEO DataSets.
func Tools(client *Client) []scimcp.RegisteredTool {
	if client == nil {
		client = NewClient()
	}
	return []scimcp.RegisteredTool{
		{
			Tool: scimcp.Tool{
				Name:        "search_datasets",
				Description: "Search NCBI GEO DataSets by keyword (anonymous)",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"query": map[string]any{"type": "string"},
						"limit": map[string]any{"type": "integer", "default": 10},
					},
					"required": []any{"query"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					Query string `json:"query"`
					Limit int    `json:"limit"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				hits, err := client.SearchDatasets(p.Query, p.Limit)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("geo", p.Query, apiVersion, hits), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "get_dataset",
				Description: "Fetch one GEO DataSet by GDS id (anonymous)",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"gds_id": map[string]any{"type": "string"},
					},
					"required": []any{"gds_id"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					GDSID string `json:"gds_id"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				row, err := client.GetDataset(p.GDSID)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("geo", p.GDSID, apiVersion, row), nil
			},
		},
	}
}
