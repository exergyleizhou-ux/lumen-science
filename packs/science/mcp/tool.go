package mcp

import (
	"context"
	"encoding/json"
)

// Tool describes one MCP tool exposed by a Lumen Science server.
type Tool struct {
	Name        string         `json:"name"`
	Description string         `json:"description"`
	InputSchema map[string]any `json:"inputSchema"`
}

// ToolHandler executes a tool and returns JSON-serializable result data.
type ToolHandler func(ctx context.Context, args json.RawMessage) (any, error)

// RegisteredTool binds metadata to a handler.
type RegisteredTool struct {
	Tool    Tool
	Handler ToolHandler
}

// TextResult wraps data as MCP tools/call text content.
func TextResult(data any) map[string]any {
	b, _ := json.MarshalIndent(data, "", "  ")
	return map[string]any{
		"content": []map[string]any{{
			"type": "text",
			"text": string(b),
		}},
	}
}

// ErrorResult returns an MCP tool error payload.
func ErrorResult(msg string) map[string]any {
	return map[string]any{
		"content": []map[string]any{{
			"type": "text",
			"text": msg,
		}},
		"isError": true,
	}
}
