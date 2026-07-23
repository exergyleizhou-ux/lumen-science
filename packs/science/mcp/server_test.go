package mcp

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
)

func TestServerDispatchToolsListAndCall(t *testing.T) {
	tools := []RegisteredTool{{
		Tool: Tool{Name: "ping", Description: "pong", InputSchema: map[string]any{"type": "object"}},
		Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
			return map[string]any{"ok": true}, nil
		},
	}}
	srv := NewServer("test", "0.1", tools)

	listReq := jsonrpcRequest{JSONRPC: "2.0", ID: json.RawMessage(`1`), Method: "tools/list"}
	listResp, err := srv.dispatch(context.Background(), listReq)
	if err != nil {
		t.Fatal(err)
	}
	if listResp.Error != nil || !strings.Contains(string(listResp.Result), "ping") {
		t.Fatalf("tools/list: %+v", listResp)
	}

	params, _ := json.Marshal(map[string]any{"name": "ping", "arguments": map[string]any{}})
	callReq := jsonrpcRequest{JSONRPC: "2.0", ID: json.RawMessage(`2`), Method: "tools/call", Params: params}
	callResp, err := srv.dispatch(context.Background(), callReq)
	if err != nil {
		t.Fatal(err)
	}
	if callResp.Error != nil || !strings.Contains(string(callResp.Result), "ok") {
		t.Fatalf("tools/call: %+v", callResp)
	}
}
