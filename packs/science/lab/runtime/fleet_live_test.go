package runtime

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func TestFleetConnectPubMedLive(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Skip(err)
	}
	sci := filepath.Join(home, ".lumen", "science")
	if _, err := os.Stat(filepath.Join(sci, "sandbox")); err != nil {
		t.Skip("no science sandbox")
	}
	fleet, err := NewFleetManager(sci)
	if err != nil {
		t.Fatal(err)
	}
	defer fleet.Close()
	if !fleet.ResearchHealthy() {
		t.Skip("research pack not healthy")
	}
	if err := fleet.ConnectDomains("pubmed"); err != nil {
		t.Fatal(err)
	}
	out, err := fleet.CallDomain(context.Background(), "pubmed", "search_articles", map[string]any{
		"query": "aspirin", "max_results": 1,
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(out) < 20 {
		t.Fatalf("short result: %q", out)
	}
}
