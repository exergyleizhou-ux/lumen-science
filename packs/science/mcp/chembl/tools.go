package chembl

import (
	"context"
	"encoding/json"

	scimcp "lumen/internal/science/mcp"
)

// Tools returns MCP registrations for ChEMBL.
func Tools(client *Client) []scimcp.RegisteredTool {
	if client == nil {
		client = NewClient()
	}
	return []scimcp.RegisteredTool{
		{
			Tool: scimcp.Tool{
				Name:        "search_compounds",
				Description: "Search ChEMBL molecules by name or synonym (anonymous)",
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
				hits, err := client.SearchCompounds(p.Query, p.Limit)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("chembl", p.Query, apiVersion, hits), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "get_molecule",
				Description: "Fetch ChEMBL molecule record by chembl_id",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"chembl_id": map[string]any{"type": "string"},
					},
					"required": []any{"chembl_id"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					ChemblID string `json:"chembl_id"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				row, err := client.GetMolecule(p.ChemblID)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("chembl", p.ChemblID, apiVersion, row), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "get_target_bioactivities",
				Description: "List bioactivities for a target ChEMBL id",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"target_chembl_id": map[string]any{"type": "string"},
						"limit":            map[string]any{"type": "integer", "default": 10},
					},
					"required": []any{"target_chembl_id"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					TargetID string `json:"target_chembl_id"`
					Limit    int    `json:"limit"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, err
				}
				rows, err := client.TargetActivities(p.TargetID, p.Limit)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("chembl", p.TargetID, apiVersion, rows), nil
			},
		},
	}
}
