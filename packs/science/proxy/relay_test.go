package proxy

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestRelaySpecDualAuth(t *testing.T) {
	spec := RelaySpec("https://relay.example.com", "key", "", nil)
	if !spec.DualAuth {
		t.Fatal("relay should enable dual auth")
	}
}

func TestFetchRelayModels(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("x-api-key") == "" || r.Header.Get("Authorization") == "" {
			w.WriteHeader(401)
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]any{
			"data": []map[string]any{{"id": "m1", "display_name": "Model One"}},
		})
	}))
	defer srv.Close()
	models, err := FetchRelayModels(context.Background(), srv.URL, "tok")
	if err != nil {
		t.Fatal(err)
	}
	if len(models) != 1 || models[0].ID != "m1" {
		t.Fatalf("%+v", models)
	}
}

func TestProbeUpstreamKey401(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(401)
	}))
	defer srv.Close()
	spec := BuiltInProviders["deepseek"]
	spec.URL = srv.URL
	code, _, err := ProbeUpstreamKey(context.Background(), spec, "bad", "")
	if err != nil {
		t.Fatal(err)
	}
	if code != 401 {
		t.Fatalf("code %d", code)
	}
}

func TestProbeUpstreamKey200(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(200)
		w.Write([]byte(`{"content":[]}`))
	}))
	defer srv.Close()
	spec := BuiltInProviders["deepseek"]
	spec.URL = srv.URL
	code, _, err := ProbeUpstreamKey(context.Background(), spec, "ok", "")
	if err != nil {
		t.Fatal(err)
	}
	if code != 200 {
		t.Fatalf("code %d", code)
	}
}
