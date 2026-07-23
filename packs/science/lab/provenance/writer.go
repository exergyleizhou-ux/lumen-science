package provenance

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"
)

// Record is one provenance.jsonl entry.
type Record struct {
	TS          time.Time `json:"ts"`
	ArtifactID  string    `json:"artifact_id"`
	Path        string    `json:"path"`
	Version     int       `json:"version"`
	Kind        string    `json:"kind"`
	SessionID   string    `json:"session_id,omitempty"`
	Model       string    `json:"model,omitempty"`
	ContentHash string    `json:"content_hash,omitempty"`
	MCPCalls    []MCPCall `json:"mcp_calls,omitempty"`
}

// MCPCall records one MCP invocation.
type MCPCall struct {
	Tool  string `json:"tool"`
	Query string `json:"query,omitempty"`
}

// Writer appends provenance records.
type Writer struct {
	path string
}

// NewWriter opens a provenance log for a project.
func NewWriter(path string) *Writer {
	return &Writer{path: path}
}

// Append records a file write.
func (w *Writer) Append(rec Record) error {
	if w == nil || w.path == "" {
		return nil
	}
	if rec.TS.IsZero() {
		rec.TS = time.Now().UTC()
	}
	data, err := json.Marshal(rec)
	if err != nil {
		return err
	}
	f, err := os.OpenFile(w.path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = f.Write(append(data, '\n'))
	return err
}

// HashFile returns sha256 of file contents.
func HashFile(path string) (string, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(b)
	return "sha256:" + hex.EncodeToString(sum[:]), nil
}

// RelPath returns path relative to project root when possible.
func RelPath(projectDir, absPath string) string {
	rel, err := filepath.Rel(projectDir, absPath)
	if err != nil {
		return absPath
	}
	return rel
}

// NextVersion counts lines in provenance file for simple versioning.
func NextVersion(path string) int {
	data, err := os.ReadFile(path)
	if err != nil || len(data) == 0 {
		return 1
	}
	n := 0
	for _, b := range data {
		if b == '\n' {
			n++
		}
	}
	if data[len(data)-1] != '\n' {
		n++
	}
	return n + 1
}

// RecordWrite builds a record for a workspace file.
func RecordWrite(projectDir, relPath, sessionID, model string) (Record, error) {
	abs := filepath.Join(projectDir, relPath)
	h, err := HashFile(abs)
	if err != nil {
		return Record{}, fmt.Errorf("hash: %w", err)
	}
	provPath := filepath.Join(projectDir, "provenance.jsonl")
	return Record{
		ArtifactID:  filepath.Base(relPath),
		Path:        relPath,
		Version:     NextVersion(provPath),
		Kind:        kindFromExt(relPath),
		SessionID:   sessionID,
		Model:       model,
		ContentHash: h,
	}, nil
}

func kindFromExt(p string) string {
	switch filepath.Ext(p) {
	case ".md":
		return "report"
	case ".png", ".jpg", ".svg":
		return "figure"
	default:
		return "artifact"
	}
}
