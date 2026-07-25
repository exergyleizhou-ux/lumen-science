// Package e2e contains end-to-end integration tests for the Lumen Science offline product loop.
//
// These tests require built MCP binaries (lumen-mcp-artifacts, lumen-mcp-notebook,
// lumen-mcp-reviewer). Build them first:
//
//	go build -o lumen-mcp-artifacts ./standalone/cmd/artifacts
//	go build -o lumen-mcp-notebook  ./standalone/cmd/notebook
//	go build -o lumen-mcp-reviewer  ./standalone/cmd/reviewer
//
// Then run:
//
//	go test ./e2e/... -v -count=1
package e2e
