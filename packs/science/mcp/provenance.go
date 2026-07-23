package mcp

import "time"

// Provenance attaches retrieval metadata to tool results.
type Provenance struct {
	Source      string    `json:"source"`
	Query       string    `json:"query,omitempty"`
	RetrievedAt time.Time `json:"retrieved_at"`
	APIVersion  string    `json:"api_version,omitempty"`
}

// WithProvenance wraps results with a provenance block.
func WithProvenance(source, query, apiVersion string, results any) map[string]any {
	return map[string]any{
		"results": results,
		"provenance": Provenance{
			Source:      source,
			Query:       query,
			RetrievedAt: time.Now().UTC(),
			APIVersion:  apiVersion,
		},
	}
}
