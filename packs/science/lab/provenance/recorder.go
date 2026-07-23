package provenance

import (
	"fmt"
	"path/filepath"
	"sync"
	"time"
)

// Recorder tracks MCP calls and appends provenance records for workspace writes.
type Recorder struct {
	mu         sync.Mutex
	writer     *Writer
	projectDir string
	sessionID  string
	model      string
}

// NewRecorder opens provenance logging for a lab project.
func NewRecorder(projectDir, sessionID, model string) (*Recorder, error) {
	if projectDir == "" {
		return nil, fmt.Errorf("project dir required")
	}
	return &Recorder{
		writer:     NewWriter(filepath.Join(projectDir, "provenance.jsonl")),
		projectDir: projectDir,
		sessionID:  sessionID,
		model:      model,
	}, nil
}

// RecordMCP appends an immediate mcp_call provenance entry (not queued).
func (r *Recorder) RecordMCP(domain, tool, query string) {
	if r == nil || r.writer == nil {
		return
	}
	provPath := filepath.Join(r.projectDir, "provenance.jsonl")
	rec := Record{
		TS:         time.Now().UTC(),
		ArtifactID: domain + "/" + tool,
		Kind:       "mcp_call",
		Version:    NextVersion(provPath),
		SessionID:  r.sessionID,
		Model:      r.model,
		MCPCalls: []MCPCall{{
			Tool:  domain + "/" + tool,
			Query: query,
		}},
	}
	_ = r.writer.Append(rec)
}

// RecordArtifact appends provenance for a file under the project.
func (r *Recorder) RecordArtifact(absPath string) error {
	if r == nil || r.writer == nil {
		return nil
	}
	rel, err := filepath.Rel(r.projectDir, absPath)
	if err != nil {
		return err
	}
	rec, err := RecordWrite(r.projectDir, rel, r.sessionID, r.model)
	if err != nil {
		return err
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.writer.Append(rec)
}
