package brief

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestGenerateAndRender(t *testing.T) {
	t.Parallel()
	mux := http.NewServeMux()
	mux.HandleFunc("/pubmed/esearch.fcgi", func(w http.ResponseWriter, r *http.Request) {
		if got := r.URL.Query().Get("term"); got != "aspirin" {
			t.Fatalf("term = %q", got)
		}
		_, _ = w.Write([]byte(`{"esearchresult":{"idlist":["123"]}}`))
	})
	mux.HandleFunc("/pubmed/esummary.fcgi", func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"result":{"uids":["123"],"123":{"title":"Aspirin trial","fulljournalname":"Example Journal","pubdate":"2026","authors":[{"name":"Lei A"}]}}}`))
	})
	mux.HandleFunc("/chembl/molecule/search.json", func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"molecules":[{"molecule_chembl_id":"CHEMBL25","pref_name":"ASPIRIN","max_phase":4}]}`))
	})
	server := httptest.NewServer(mux)
	t.Cleanup(server.Close)

	client := NewClient(time.Second)
	client.pubMedBase = server.URL + "/pubmed"
	client.chemblBase = server.URL + "/chembl"
	client.now = func() time.Time { return time.Date(2026, 7, 16, 12, 0, 0, 0, time.UTC) }
	result, err := client.Generate(context.Background(), " aspirin ", 5, 3)
	if err != nil {
		t.Fatal(err)
	}
	if len(result.Articles) != 1 || result.Articles[0].PMID != "123" {
		t.Fatalf("articles = %#v", result.Articles)
	}
	if len(result.Compounds) != 1 || result.Compounds[0].ID != "CHEMBL25" {
		t.Fatalf("compounds = %#v", result.Compounds)
	}
	markdown := result.Markdown()
	for _, want := range []string{"PMID 123", "CHEMBL25", "2026-07-16T12:00:00Z", "no medical advice"} {
		if !strings.Contains(markdown, want) {
			t.Errorf("Markdown missing %q:\n%s", want, markdown)
		}
	}
}

func TestChEMBLFailureIsVisibleButNonFatal(t *testing.T) {
	t.Parallel()
	mux := http.NewServeMux()
	mux.HandleFunc("/pubmed/esearch.fcgi", func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"esearchresult":{"idlist":[]}}`))
	})
	mux.HandleFunc("/chembl/molecule/search.json", func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "unavailable", http.StatusServiceUnavailable)
	})
	server := httptest.NewServer(mux)
	t.Cleanup(server.Close)

	client := NewClient(time.Second)
	client.pubMedBase = server.URL + "/pubmed"
	client.chemblBase = server.URL + "/chembl"
	result, err := client.Generate(context.Background(), "topic", 1, 1)
	if err != nil {
		t.Fatal(err)
	}
	if len(result.Warnings) != 1 || !strings.Contains(result.Warnings[0], "HTTP 503") {
		t.Fatalf("warnings = %#v", result.Warnings)
	}
}

func TestPubMedFailureFailsClosed(t *testing.T) {
	t.Parallel()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "rate limited", http.StatusTooManyRequests)
	}))
	t.Cleanup(server.Close)

	client := NewClient(time.Second)
	client.pubMedBase = server.URL
	client.chemblBase = server.URL
	client.ncbiAPIKey = "super-secret-test-key"
	_, err := client.Generate(context.Background(), "topic", 1, 1)
	if err == nil || !strings.Contains(err.Error(), "HTTP 429") {
		t.Fatalf("err = %v", err)
	}
	if strings.Contains(err.Error(), client.ncbiAPIKey) || !strings.Contains(err.Error(), "api_key=REDACTED") {
		t.Fatalf("API key was not redacted: %v", err)
	}
}
