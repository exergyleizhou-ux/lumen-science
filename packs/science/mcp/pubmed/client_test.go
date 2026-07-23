package pubmed

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestSearchArticlesEsummary(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case strings.Contains(r.URL.Path, "esearch"):
			w.Write([]byte(`{"esearchresult":{"idlist":["12345"]}}`))
		case strings.Contains(r.URL.Path, "esummary"):
			w.Write([]byte(`{"result":{"12345":{"title":"Aspirin study","source":"J Test","pubdate":"2024 Jan","authors":[{"name":"Smith J"}],"articleids":[{"idtype":"doi","value":"10.1/test"}]}}}`))
		default:
			http.NotFound(w, r)
		}
	}))
	defer srv.Close()

	c := NewClient()
	c.BaseURL = srv.URL
	rows, err := c.SearchArticles("aspirin", 1)
	if err != nil {
		t.Fatal(err)
	}
	if len(rows) != 1 || rows[0].PMID != "12345" || rows[0].DOI != "10.1/test" {
		t.Fatalf("rows: %+v", rows)
	}
}
