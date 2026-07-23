// Package mcp implements a minimal MCP 2024-11-05 stdio server for Lumen Science.
package mcp

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"strconv"
	"strings"
)

type jsonrpcRequest struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
}

type jsonrpcNotification struct {
	JSONRPC string          `json:"jsonrpc"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
}

type jsonrpcResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  json.RawMessage `json:"result,omitempty"`
	Error   *jsonrpcError   `json:"error,omitempty"`
}

type jsonrpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func writeMessage(w io.Writer, v any) error {
	body, err := json.Marshal(v)
	if err != nil {
		return err
	}
	if _, err := fmt.Fprintf(w, "Content-Length: %d\r\n\r\n", len(body)); err != nil {
		return err
	}
	_, err = w.Write(body)
	return err
}

func readMessage(r *bufio.Reader) (json.RawMessage, bool, error) {
	headerLine, err := r.ReadString('\n')
	if err != nil {
		return nil, false, err
	}
	headerLine = strings.TrimSpace(headerLine)
	if !strings.HasPrefix(headerLine, "Content-Length:") {
		return nil, false, fmt.Errorf("expected Content-Length, got %q", headerLine)
	}
	lengthStr := strings.TrimSpace(strings.TrimPrefix(headerLine, "Content-Length:"))
	length, err := strconv.Atoi(lengthStr)
	if err != nil {
		return nil, false, err
	}
	if _, err := r.ReadString('\n'); err != nil {
		return nil, false, err
	}
	body := make([]byte, length)
	if _, err := io.ReadFull(r, body); err != nil {
		return nil, false, err
	}
	var probe struct {
		ID json.RawMessage `json:"id"`
	}
	if err := json.Unmarshal(body, &probe); err != nil {
		return nil, false, err
	}
	isRequest := len(probe.ID) > 0 && string(probe.ID) != "null"
	return body, isRequest, nil
}
