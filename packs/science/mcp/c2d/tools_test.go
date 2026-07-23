package c2d

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestListAlgorithmsToolWithToken(t *testing.T) {
	mock := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer c2d-tok" {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		w.Write([]byte(`{"code":0,"data":{"algorithms":[{"id":"algo1"}]}}`))
	}))
	defer mock.Close()

	tools := Tools(Config{BaseURL: mock.URL, Token: "c2d-tok"})
	var handler func(context.Context, json.RawMessage) (any, error)
	for _, reg := range tools {
		if reg.Tool.Name == "list_algorithms" {
			handler = reg.Handler
			break
		}
	}
	if handler == nil {
		t.Fatal("list_algorithms tool missing")
	}
	out, err := handler(context.Background(), json.RawMessage(`{"dataset_id":"ds1"}`))
	if err != nil {
		t.Fatal(err)
	}
	m, ok := out.(map[string]any)
	if !ok || m["provenance"] == nil {
		t.Fatalf("missing provenance: %v", out)
	}
}
