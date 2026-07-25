package artifacts

import "github.com/lumen-ai/lumen-science/mcp"

// Run starts the Artifacts MCP server on stdio.
func Run() error {
	store, err := NewStore()
	if err != nil {
		return err
	}
	server := mcp.NewServer("lumen-science-artifacts", "0.1.0", Tools(store))
	return server.RunStdio()
}
