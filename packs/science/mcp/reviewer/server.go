package reviewer

import (
	"os"
	"path/filepath"

	"github.com/lumen-ai/lumen-science/mcp"
)

// Server wires the Reviewer to the MCP transport.
type Server struct {
	Reviewer *Reviewer
}

// Run starts the Reviewer MCP server on stdio.
func Run() error {
	home, _ := os.UserHomeDir()
	root := filepath.Join(home, ".lumen", "science", "artifacts")
	r := NewReviewer(root)
	srv := &Server{Reviewer: r}
	server := mcp.NewServer("lumen-science-reviewer", "0.1.0", Tools(srv))
	return server.RunStdio()
}
