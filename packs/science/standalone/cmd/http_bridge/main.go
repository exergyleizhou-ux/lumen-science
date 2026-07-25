// Command http_bridge exposes a stdio MCP server as an HTTP endpoint.
//
// Configuration via environment:
//
//	BRIDGE_TARGET_COMMAND  — path to the MCP binary (required)
//	BRIDGE_TARGET_ARGS     — space-separated arguments (optional)
//	BRIDGE_BEARER_TOKEN    — Bearer token for auth (required)
//	BRIDGE_PORT            — listen port (default 9090)
//
// Example:
//
//	BRIDGE_TARGET_COMMAND=./lumen-mcp-artifacts \
//	BRIDGE_BEARER_TOKEN=secret \
//	BRIDGE_PORT=9090 \
//	  lumen-http-bridge
package main

import (
	"fmt"
	"os"

	"github.com/lumen-ai/lumen-science/mcp/http_bridge"
)

func main() {
	if err := http_bridge.Run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
