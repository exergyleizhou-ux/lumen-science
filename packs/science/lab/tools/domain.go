package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"lumen/internal/science/lab/runtime"
	"lumen/internal/tool"
)

// MCPRecorder records domain MCP calls for provenance.
type MCPRecorder interface {
	RecordMCP(domain, tool, query string)
}

// DomainCallTool invokes a bio-tools domain MCP tool.
type DomainCallTool struct {
	Fleet      *runtime.FleetManager
	Provenance MCPRecorder
}

func (t *DomainCallTool) Name() string { return "science_domain_call" }

func (t *DomainCallTool) ReadOnly() bool { return true }

func (t *DomainCallTool) Description() string {
	return "Call a Claude Science bio-tools domain MCP tool. Domains include pubmed, literature, chembl, expression, chemistry, etc. Use science_list_domains first."
}

func (t *DomainCallTool) Schema() json.RawMessage {
	return json.RawMessage(`{"type":"object","properties":{"domain":{"type":"string","description":"Domain slug, e.g. pubmed, literature, chembl"},"tool":{"type":"string","description":"Tool name within the domain"},"arguments":{"type":"object","description":"Tool arguments object"}},"required":["domain","tool"]}`)
}

func (t *DomainCallTool) Execute(ctx context.Context, args json.RawMessage) (string, error) {
	var p struct {
		Domain    string         `json:"domain"`
		Tool      string         `json:"tool"`
		Arguments map[string]any `json:"arguments"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return "", err
	}
	domain := strings.TrimSpace(p.Domain)
	toolName := strings.TrimSpace(p.Tool)
	if domain == "" || toolName == "" {
		return "", fmt.Errorf("domain and tool are required")
	}
	if t.Fleet == nil {
		return "", fmt.Errorf("science fleet unavailable — run lumen science start once to clone research pack")
	}
	if p.Arguments == nil {
		p.Arguments = map[string]any{}
	}
	if t.Provenance != nil {
		q, _ := json.Marshal(p.Arguments)
		t.Provenance.RecordMCP(domain, toolName, string(q))
	}
	return t.Fleet.CallDomain(ctx, domain, toolName, p.Arguments)
}

// ListDomainsTool lists bio-tools domains from the research pack.
type ListDomainsTool struct {
	Fleet *runtime.FleetManager
}

func (t *ListDomainsTool) Name() string { return "science_list_domains" }

func (t *ListDomainsTool) ReadOnly() bool { return true }

func (t *ListDomainsTool) Description() string {
	return "List available Claude Science bio-tools domains and tool counts from the local research pack."
}

func (t *ListDomainsTool) Schema() json.RawMessage {
	return json.RawMessage(`{"type":"object","properties":{}}`)
}

func (t *ListDomainsTool) Execute(ctx context.Context, args json.RawMessage) (string, error) {
	if t.Fleet == nil {
		return "", fmt.Errorf("science fleet unavailable")
	}
	domains := t.Fleet.Domains()
	b, err := json.MarshalIndent(domains, "", "  ")
	if err != nil {
		return "", err
	}
	return string(b), nil
}

// RegisterFleet returns lab-specific agent tools.
func RegisterFleet(fleet *runtime.FleetManager, rec MCPRecorder) []tool.Tool {
	return []tool.Tool{
		&DomainCallTool{Fleet: fleet, Provenance: rec},
		&ListDomainsTool{Fleet: fleet},
	}
}
