package geo

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestSearchDatasetsToolProvenance(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/esearch.fcgi":
			w.Write([]byte(`{"esearchresult":{"idlist":["2001"]}}`))
		case "/esummary.fcgi":
			w.Write([]byte(`{"result":{"2001":{"accession":"GDS99","title":"GEO test","taxon":"mouse","n_samples":5}}}`))
		default:
			http.NotFound(w, r)
		}
	}))
	defer srv.Close()

	client := &Client{BaseURL: srv.URL, HTTPClient: srv.Client(), UserAgent: "test"}
	tools := Tools(client)
	var handler func(context.Context, json.RawMessage) (any, error)
	for _, reg := range tools {
		if reg.Tool.Name == "search_datasets" {
			handler = reg.Handler
			break
		}
	}
	if handler == nil {
		t.Fatal("search_datasets tool missing")
	}
	out, err := handler(context.Background(), json.RawMessage(`{"query":"expression","limit":1}`))
	if err != nil {
		t.Fatal(err)
	}
	m, ok := out.(map[string]any)
	if !ok {
		t.Fatalf("type %T", out)
	}
	if m["provenance"] == nil {
		t.Fatal("missing provenance")
	}
	results, ok := m["results"].([]DatasetHit)
	if !ok || len(results) != 1 || results[0].Accession != "GDS99" {
		t.Fatalf("results: %v", m["results"])
	}
}
