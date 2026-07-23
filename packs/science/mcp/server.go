package mcp

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
)

// Server is a stdio MCP host for Lumen Science native tools.
type Server struct {
	Name    string
	Version string
	tools   map[string]RegisteredTool
}

// NewServer builds an MCP server from registered tools.
func NewServer(name, version string, tools []RegisteredTool) *Server {
	m := make(map[string]RegisteredTool, len(tools))
	for _, t := range tools {
		m[t.Tool.Name] = t
	}
	return &Server{Name: name, Version: version, tools: m}
}

// RunStdio serves MCP over os.Stdin/os.Stdout until EOF.
func (s *Server) RunStdio() error {
	return s.Serve(os.Stdin, os.Stdout)
}

// Serve handles MCP JSON-RPC on the given streams.
func (s *Server) Serve(r io.Reader, w io.Writer) error {
	br := bufio.NewReader(r)
	for {
		body, isRequest, err := readMessage(br)
		if err != nil {
			if err == io.EOF {
				return nil
			}
			return err
		}
		if !isRequest {
			var n jsonrpcNotification
			if err := json.Unmarshal(body, &n); err != nil {
				return err
			}
			continue
		}
		var req jsonrpcRequest
		if err := json.Unmarshal(body, &req); err != nil {
			return err
		}
		resp, err := s.dispatch(context.Background(), req)
		if err != nil {
			return err
		}
		if err := writeMessage(w, resp); err != nil {
			return err
		}
	}
}

func (s *Server) dispatch(ctx context.Context, req jsonrpcRequest) (jsonrpcResponse, error) {
	resp := jsonrpcResponse{JSONRPC: "2.0", ID: req.ID}
	switch req.Method {
	case "initialize":
		result, _ := json.Marshal(map[string]any{
			"protocolVersion": "2024-11-05",
			"capabilities": map[string]any{
				"tools": map[string]any{},
			},
			"serverInfo": map[string]any{
				"name":    s.Name,
				"version": s.Version,
			},
		})
		resp.Result = result
	case "tools/list":
		list := make([]Tool, 0, len(s.tools))
		for _, t := range s.tools {
			list = append(list, t.Tool)
		}
		result, _ := json.Marshal(map[string]any{"tools": list})
		resp.Result = result
	case "tools/call":
		var params struct {
			Name      string          `json:"name"`
			Arguments json.RawMessage `json:"arguments"`
		}
		if err := json.Unmarshal(req.Params, &params); err != nil {
			resp.Error = &jsonrpcError{Code: -32602, Message: "invalid params"}
			return resp, nil
		}
		reg, ok := s.tools[params.Name]
		if !ok {
			resp.Error = &jsonrpcError{Code: -32601, Message: fmt.Sprintf("unknown tool %q", params.Name)}
			return resp, nil
		}
		data, err := reg.Handler(ctx, params.Arguments)
		if err != nil {
			result, _ := json.Marshal(ErrorResult(err.Error()))
			resp.Result = result
			return resp, nil
		}
		result, _ := json.Marshal(TextResult(data))
		resp.Result = result
	default:
		resp.Error = &jsonrpcError{Code: -32601, Message: fmt.Sprintf("method not found: %s", req.Method)}
	}
	return resp, nil
}
