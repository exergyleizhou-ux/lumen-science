package pubmed

import (
	"context"
	"encoding/json"
	"fmt"

	scimcp "lumen/internal/science/mcp"
)

// Tools returns MCP tool registrations for PubMed.
func Tools(client *Client) []scimcp.RegisteredTool {
	if client == nil {
		client = NewClient()
	}
	return []scimcp.RegisteredTool{
		{
			Tool: scimcp.Tool{
				Name:        "search_articles",
				Description: "Search PubMed; returns PMIDs, titles, authors, journal, year, DOI",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"query":       map[string]any{"type": "string"},
						"max_results": map[string]any{"type": "integer", "default": 20},
					},
					"required": []any{"query"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					Query      string `json:"query"`
					MaxResults int    `json:"max_results"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, fmt.Errorf("invalid args: %w", err)
				}
				rows, err := client.SearchArticles(p.Query, p.MaxResults)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("pubmed", p.Query, apiVersion, rows), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "fetch_abstract",
				Description: "Fetch title and abstract for a PubMed ID (PMID)",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"pmid": map[string]any{"type": "string"},
					},
					"required": []any{"pmid"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					PMID string `json:"pmid"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, fmt.Errorf("invalid args: %w", err)
				}
				row, err := client.FetchAbstract(p.PMID)
				if err != nil {
					return nil, err
				}
				return scimcp.WithProvenance("pubmed", "pmid:"+p.PMID, apiVersion, row), nil
			},
		},
		{
			Tool: scimcp.Tool{
				Name:        "fetch_pmc_fulltext",
				Description: "Check PMC open-access availability for a PMID (metadata only in Phase 1)",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"pmid": map[string]any{"type": "string"},
					},
					"required": []any{"pmid"},
				},
			},
			Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
				var p struct {
					PMID string `json:"pmid"`
				}
				if err := json.Unmarshal(args, &p); err != nil {
					return nil, fmt.Errorf("invalid args: %w", err)
				}
				// Phase 1: link-out metadata; full text ingestion lands in Phase 2.
				row := map[string]any{
					"pmid":       p.PMID,
					"pmc_status": "check_via_pmc",
					"pmc_url":    fmt.Sprintf("https://www.ncbi.nlm.nih.gov/pmc/articles/pmid/%s/", p.PMID),
					"note":       "full-text download pipeline scheduled for Phase 2",
				}
				return scimcp.WithProvenance("pubmed", "pmid:"+p.PMID, apiVersion, row), nil
			},
		},
	}
}
