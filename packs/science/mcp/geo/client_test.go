package geo

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestSearchDatasets(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case r.URL.Path == "/esearch.fcgi":
			w.Write([]byte(`{"esearchresult":{"idlist":["200123456"]}}`))
		case r.URL.Path == "/esummary.fcgi":
			w.Write([]byte(`{"result":{"200123456":{"accession":"GDS1234","title":"Test GEO","taxon":"Homo sapiens","n_samples":"12","platform":"GPL96"}}}`))
		default:
			http.NotFound(w, r)
		}
	}))
	defer srv.Close()

	c := &Client{BaseURL: srv.URL, HTTPClient: srv.Client(), UserAgent: "test"}
	hits, err := c.SearchDatasets("aspirin", 1)
	if err != nil {
		t.Fatal(err)
	}
	if len(hits) != 1 || hits[0].Accession != "GDS1234" || hits[0].Samples != 12 {
		t.Fatalf("hits: %+v", hits)
	}
}

func TestSearchDatasetsEmptyQuery(t *testing.T) {
	c := NewClient()
	_, err := c.SearchDatasets("  ", 5)
	if err == nil {
		t.Fatal("expected error")
	}
}
