// Command artifacts is the Lumen Science MCP server for durable artifact storage.
package main

import (
	"fmt"
	"os"

	"github.com/lumen-ai/lumen-science/mcp/artifacts"
)

func main() {
	if err := artifacts.Run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
