package native

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"
)

// LiveCheck is one smoke test against a native MCP tool.
type LiveCheck struct {
	FleetID string
	Tool    string
	Args    map[string]any
	Timeout time.Duration
}

// DefaultLiveChecks are Phase 1 smoke tests for shipped fleet members.
var DefaultLiveChecks = []LiveCheck{
	{FleetID: "pubmed", Tool: "search_articles", Args: map[string]any{"query": "aspirin", "max_results": 1}, Timeout: 20 * time.Second},
	{FleetID: "chembl", Tool: "search_compounds", Args: map[string]any{"query": "aspirin", "limit": 1}, Timeout: 20 * time.Second},
	{FleetID: "oasis", Tool: "search_datasets", Args: map[string]any{"q": "gene", "limit": 2}, Timeout: 20 * time.Second},
	{FleetID: "c2d", Tool: "list_offer_signals", Args: map[string]any{}, Timeout: 20 * time.Second},
	{FleetID: "geo", Tool: "search_datasets", Args: map[string]any{"query": "gene expression", "limit": 1}, Timeout: 25 * time.Second},
}

// LiveResult is the outcome of one live check.
type LiveResult struct {
	FleetID string `json:"fleet_id"`
	Tool    string `json:"tool"`
	Pass    bool   `json:"pass"`
	Error   string `json:"error,omitempty"`
	Snippet string `json:"snippet,omitempty"`
}

// VerifyLive runs smoke tool calls against the shipped native fleet.
func VerifyLive(ctx context.Context, checks []LiveCheck) ([]LiveResult, error) {
	mgr := NewManager()
	defer mgr.Close()
	if _, err := mgr.ConnectAll(); err != nil {
		return nil, err
	}
	return VerifyLiveWithManager(ctx, mgr, checks)
}

// VerifyLiveFromDir loads science config (oasis token injection) then verifies.
func VerifyLiveFromDir(ctx context.Context, sciDir string, checks []LiveCheck) ([]LiveResult, error) {
	mgr, err := NewManagerFromDir(sciDir)
	if err != nil {
		return nil, err
	}
	defer mgr.Close()
	if _, err := mgr.ConnectAll(); err != nil {
		return nil, err
	}
	return VerifyLiveWithManager(ctx, mgr, checks)
}

// VerifyLiveWithManager runs checks on an already-connected manager.
func VerifyLiveWithManager(ctx context.Context, mgr *Manager, checks []LiveCheck) ([]LiveResult, error) {
	if len(checks) == 0 {
		checks = DefaultLiveChecks
	}
	results := make([]LiveResult, 0, len(checks))
	for _, chk := range checks {
		res := LiveResult{FleetID: chk.FleetID, Tool: chk.Tool}
		cctx, cancel := context.WithTimeout(ctx, chk.Timeout)
		text, err := mgr.CallToolCtx(cctx, chk.FleetID, chk.Tool, chk.Args)
		cancel()
		if err != nil {
			res.Error = err.Error()
		} else {
			var probe struct {
				Provenance map[string]any `json:"provenance"`
			}
			if json.Unmarshal([]byte(text), &probe) != nil || probe.Provenance == nil {
				res.Pass = false
				res.Error = "missing provenance block"
			} else {
				res.Pass = true
			}
			snip := text
			if len(snip) > 400 {
				snip = snip[:400] + "…"
			}
			res.Snippet = snip
		}
		results = append(results, res)
	}
	return results, nil
}

// VerifyLiveReport formats live check results for CLI output.
func VerifyLiveReport(results []LiveResult) (bool, string) {
	allOK := true
	var b strings.Builder
	for _, r := range results {
		if r.Pass {
			b.WriteString(fmt.Sprintf("✓ %s.%s\n", r.FleetID, r.Tool))
		} else {
			allOK = false
			b.WriteString(fmt.Sprintf("✗ %s.%s — %s\n", r.FleetID, r.Tool, r.Error))
		}
	}
	if allOK {
		b.WriteString("✓ native MCP live verify PASS\n")
	} else {
		b.WriteString("✗ native MCP live verify FAIL\n")
	}
	return allOK, b.String()
}

// AllLiveChecksPass reports whether every live result passed.
func AllLiveChecksPass(results []LiveResult) bool {
	for _, r := range results {
		if !r.Pass {
			return false
		}
	}
	return len(results) > 0
}
