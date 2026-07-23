package chembl

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestSearchCompounds(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/molecule/search" {
			t.Fatalf("path %s", r.URL.Path)
		}
		if r.Method != http.MethodGet {
			t.Fatalf("method %s", r.Method)
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"molecules":[{"molecule_chembl_id":"CHEMBL25","pref_name":"ASPIRIN","max_phase":"4.0"}]}`))
	}))
	defer srv.Close()

	c := &Client{BaseURL: srv.URL, HTTPClient: srv.Client()}
	hits, err := c.SearchCompounds("aspirin", 1)
	if err != nil {
		t.Fatal(err)
	}
	if len(hits) != 1 || hits[0].ChemblID != "CHEMBL25" || hits[0].MaxPhase != 4 {
		t.Fatalf("hits: %+v", hits)
	}
}

func TestSearchCompoundsEmptyQuery(t *testing.T) {
	c := NewClient()
	_, err := c.SearchCompounds("  ", 5)
	if err == nil {
		t.Fatal("expected error")
	}
}
