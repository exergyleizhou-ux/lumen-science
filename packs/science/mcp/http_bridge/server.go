package http_bridge

import (
	"fmt"
	"net/http"
	"os"
	"strings"
)

// Run starts the HTTP Bridge server. Configuration comes from environment:
//
//	BRIDGE_TARGET_COMMAND  — path to the MCP binary (required)
//	BRIDGE_TARGET_ARGS     — space-separated arguments (optional)
//	BRIDGE_BEARER_TOKEN    — Bearer token for auth (required)
//	BRIDGE_PORT            — listen port (default 9090)
func Run() error {
	command := os.Getenv("BRIDGE_TARGET_COMMAND")
	if command == "" {
		return fmt.Errorf("BRIDGE_TARGET_COMMAND not set")
	}
	token := os.Getenv("BRIDGE_BEARER_TOKEN")
	if token == "" {
		return fmt.Errorf("BRIDGE_BEARER_TOKEN not set")
	}
	var args []string
	if raw := os.Getenv("BRIDGE_TARGET_ARGS"); raw != "" {
		args = strings.Fields(raw)
	}
	port := os.Getenv("BRIDGE_PORT")
	if port == "" {
		port = "9090"
	}

	bridge := NewBridge(command, args, token)
	if err := bridge.Start(); err != nil {
		return fmt.Errorf("start bridge: %w", err)
	}
	defer bridge.Shutdown()

	mux := http.NewServeMux()
	mux.HandleFunc("/tools/call", bridge.ServeHTTP)
	mux.HandleFunc("/tools/list", bridge.ServeHTTP)
	mux.HandleFunc("/health", bridge.HealthHandler)

	addr := ":" + port
	fmt.Fprintf(os.Stderr, "lumen-http-bridge: listening on %s (target=%s)\n", addr, command)
	return http.ListenAndServe(addr, mux)
}
