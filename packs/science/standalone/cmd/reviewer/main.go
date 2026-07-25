// Command reviewer is the Lumen Science MCP server for artifact review.
package main

import (
	"fmt"
	"os"

	"github.com/lumen-ai/lumen-science/mcp/reviewer"
)

func main() {
	if err := reviewer.Run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
