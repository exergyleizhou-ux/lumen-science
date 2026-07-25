package notebook

import "github.com/lumen-ai/lumen-science/mcp"

// Server wires the notebook kernel to the MCP transport.
type Server struct {
	Kernel *Kernel
}

// Run starts the notebook MCP server on stdio.
func Run() error {
	srv := &Server{Kernel: &Kernel{}}
	server := mcp.NewServer("lumen-science-notebook", "0.1.0", Tools(srv))
	return server.RunStdio()
}
