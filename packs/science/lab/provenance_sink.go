package lab

import (
	"encoding/json"
	"strings"
	"sync"

	"lumen/internal/event"
	"lumen/internal/science/lab/provenance"
	"lumen/internal/science/lab/workspace"
)

// provenanceSink records MCP tool results and workspace writes into provenance.jsonl.
type provenanceSink struct {
	inner   event.Sink
	rec     *provenance.Recorder
	guard   *workspace.Guard
	pending map[string]string
	mu      sync.Mutex
}

func wrapProvenanceSink(inner event.Sink, rec *provenance.Recorder, guard *workspace.Guard) event.Sink {
	if inner == nil || rec == nil {
		return inner
	}
	return &provenanceSink{inner: inner, rec: rec, guard: guard, pending: map[string]string{}}
}

func (s *provenanceSink) Emit(e event.Event) {
	switch e.Kind {
	case event.ToolDispatch:
		if path := writePathFromArgs(e.Tool.Name, e.Tool.Args); path != "" {
			s.mu.Lock()
			s.pending[e.Tool.ID] = path
			s.mu.Unlock()
		}
	case event.ToolResult:
		if e.Tool.Err == "" {
			s.recordWriteResult(e.Tool.ID, e.Tool.Name, e.Tool.Output)
		}
	}
	s.inner.Emit(e)
}

func (s *provenanceSink) recordWriteResult(id, name, output string) {
	s.mu.Lock()
	path := s.pending[id]
	delete(s.pending, id)
	s.mu.Unlock()
	if path == "" {
		path = writePathFromOutput(name, output)
	}
	if path == "" || s.guard == nil {
		return
	}
	abs, err := s.guard.Resolve(path)
	if err != nil {
		return
	}
	_ = s.rec.RecordArtifact(abs)
}

var writeTools = map[string]bool{
	"write_file": true, "edit_file": true, "multi_edit": true,
	"notebook_edit": true, "science_brief_generate": true,
}

func writePathFromArgs(toolName, argsJSON string) string {
	if !writeTools[toolName] {
		return ""
	}
	switch toolName {
	case "science_brief_generate":
		return "reports/brief.md"
	case "multi_edit":
		var p struct {
			Edits []struct {
				Path string `json:"path"`
			} `json:"edits"`
		}
		if json.Unmarshal([]byte(argsJSON), &p) == nil && len(p.Edits) > 0 {
			return p.Edits[0].Path
		}
	}
	var p struct {
		Path string `json:"path"`
	}
	if json.Unmarshal([]byte(argsJSON), &p) == nil && p.Path != "" {
		return p.Path
	}
	return ""
}

func writePathFromOutput(toolName, output string) string {
	if toolName == "write_file" && strings.Contains(output, " to ") {
		parts := strings.SplitN(output, " to ", 2)
		if len(parts) == 2 {
			return strings.TrimSpace(parts[1])
		}
	}
	return ""
}
