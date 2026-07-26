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

// TestServerDispatchPreservesErrorResultIsError ensures handlers that return
// ErrorResult maps keep top-level isError (no double TextResult wrap).
func TestServerDispatchPreservesErrorResultIsError(t *testing.T) {
	tools := []RegisteredTool{{
		Tool: Tool{Name: "deny", Description: "reject", InputSchema: map[string]any{"type": "object"}},
		Handler: func(ctx context.Context, args json.RawMessage) (any, error) {
			return ErrorResult("invalid path: \"../../etc/passwd\""), nil
		},
	}}
	srv := NewServer("test", "0.1", tools)
	params, _ := json.Marshal(map[string]any{"name": "deny", "arguments": map[string]any{}})
	callReq := jsonrpcRequest{JSONRPC: "2.0", ID: json.RawMessage(`3`), Method: "tools/call", Params: params}
	callResp, err := srv.dispatch(context.Background(), callReq)
	if err != nil {
		t.Fatal(err)
	}
	if callResp.Error != nil {
		t.Fatalf("unexpected rpc error: %+v", callResp.Error)
	}
	var result map[string]any
	if err := json.Unmarshal(callResp.Result, &result); err != nil {
		t.Fatalf("unmarshal: %v raw=%s", err, callResp.Result)
	}
	isErr, _ := result["isError"].(bool)
	if !isErr {
		t.Fatalf("expected top-level isError=true, got: %s", callResp.Result)
	}
	if !strings.Contains(string(callResp.Result), "invalid path") {
		t.Fatalf("expected invalid path message, got: %s", callResp.Result)
	}
}
