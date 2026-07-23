package oasis

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestSearchDatasetsAnonymous(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "" {
			t.Fatal("search should be anonymous")
		}
		if r.URL.Path != "/api/v1/datasets" {
			http.NotFound(w, r)
			return
		}
		w.Write([]byte(`{"code":0,"message":"ok","data":{"items":[{"id":"ds1","title":"Test DS","quality_verified":true}]}}`))
	}))
	defer srv.Close()

	c := NewClient(srv.URL, "")
	items, err := c.SearchDatasets("gene", 10, 0)
	if err != nil {
		t.Fatal(err)
	}
	if len(items) != 1 || items[0].ID != "ds1" {
		t.Fatalf("items: %+v", items)
	}
}

func TestPreviewRequiresToken(t *testing.T) {
	c := NewClient("http://example.com", "")
	_, err := c.PreviewSchema("ds1")
	if err == nil {
		t.Fatal("expected auth error")
	}
}

func TestPreviewWithToken(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer test-token" {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		w.Write([]byte(`{"code":0,"message":"ok","data":{"lines":["a,b,c","1,2,3"],"line_count":2,"dataset_sample_count":100,"truncated":true}}`))
	}))
	defer srv.Close()

	c := NewClient(srv.URL, "test-token")
	row, err := c.PreviewSchema("ds1")
	if err != nil {
		t.Fatal(err)
	}
	cols, _ := row["columns"].([]string)
	if len(cols) != 3 {
		t.Fatalf("columns: %v", row["columns"])
	}
}

func TestCheckAuthPolicy(t *testing.T) {
	if err := CheckAuth("search_datasets", ""); err != nil {
		t.Fatal(err)
	}
	if err := CheckAuth("preview_schema", ""); err == nil {
		t.Fatal("preview should require token")
	}
}
