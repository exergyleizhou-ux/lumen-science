package tools

import (
	"context"
	"encoding/json"
	"path/filepath"
	"testing"

	labruntime "lumen/internal/science/lab/runtime"
)

type mockProv struct {
	calls []string
}

func (m *mockProv) RecordMCP(domain, tool, query string) {
	m.calls = append(m.calls, domain+"/"+tool+":"+query)
}

func TestDomainCallRecordsProvenance(t *testing.T) {
	home := t.TempDir()
	sci := filepath.Join(home, ".lumen", "science")
	fleet, err := labruntime.NewFleetManager(sci)
	if err != nil {
		t.Fatal(err)
	}
	prov := &mockProv{}
	tool := &DomainCallTool{Fleet: fleet, Provenance: prov}
	_, err = tool.Execute(context.Background(), json.RawMessage(`{"domain":"pubmed","tool":"search_articles","arguments":{"query":"x"}}`))
	if err == nil {
		t.Skip("fleet live not available in this env")
	}
	// even on fleet error, RecordMCP should have been called before call
	if len(prov.calls) != 1 {
		t.Fatalf("RecordMCP calls = %v", prov.calls)
	}
}

func TestListDomainsTool(t *testing.T) {
	home := t.TempDir()
	sci := filepath.Join(home, ".lumen", "science")
	fleet, err := labruntime.NewFleetManager(sci)
	if err != nil {
		t.Fatal(err)
	}
	tool := &ListDomainsTool{Fleet: fleet}
	out, err := tool.Execute(context.Background(), json.RawMessage(`{}`))
	if err != nil {
		t.Fatal(err)
	}
	if out == "" {
		t.Fatal("empty output")
	}
}
