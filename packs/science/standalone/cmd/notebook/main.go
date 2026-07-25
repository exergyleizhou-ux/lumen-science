// Command notebook is the Lumen Science MCP server for Python notebook execution.
package main

import (
	"fmt"
	"os"

	"github.com/lumen-ai/lumen-science/mcp/notebook"
)

func main() {
	if err := notebook.Run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
