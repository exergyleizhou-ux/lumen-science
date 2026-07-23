package c2d

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestListOfferSignalsAnonymous(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "" {
			t.Fatal("signals should be anonymous")
		}
		if r.URL.Path != "/api/v1/compute/offers/signals" {
			http.NotFound(w, r)
			return
		}
		w.Write([]byte(`{"code":0,"data":{"signals":[{"dataset_id":"ds1"}]}}`))
	}))
	defer srv.Close()

	c := NewClient(srv.URL, "")
	data, err := c.ListOfferSignals()
	if err != nil {
		t.Fatal(err)
	}
	m, ok := data.(map[string]any)
	if !ok {
		t.Fatalf("type %T", data)
	}
	if _, ok := m["signals"]; !ok {
		t.Fatalf("data: %v", m)
	}
}

func TestListAlgorithmsRequiresToken(t *testing.T) {
	c := NewClient("http://example.com", "")
	_, err := c.ListAlgorithms("ds1")
	if err == nil {
		t.Fatal("expected auth error")
	}
}

func TestListAlgorithmsWithToken(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer user-tok" {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		if r.URL.Path != "/api/v1/compute/algorithms" {
			http.NotFound(w, r)
			return
		}
		w.Write([]byte(`{"code":0,"data":{"algorithms":[{"id":"algo1","name":"Causal DAG"}]}}`))
	}))
	defer srv.Close()

	c := NewClient(srv.URL, "user-tok")
	data, err := c.ListAlgorithms("ds1")
	if err != nil {
		t.Fatal(err)
	}
	m, ok := data.(map[string]any)
	if !ok {
		t.Fatalf("type %T", data)
	}
	algs, ok := m["algorithms"].([]any)
	if !ok || len(algs) != 1 {
		t.Fatalf("algorithms: %v", m["algorithms"])
	}
}
