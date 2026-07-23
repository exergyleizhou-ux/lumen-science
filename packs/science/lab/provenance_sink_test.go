package lab

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"lumen/internal/event"
	"lumen/internal/science/lab/provenance"
	"lumen/internal/science/lab/workspace"
)

func TestProvenanceSinkWriteFile(t *testing.T) {
	dir := t.TempDir()
	ws := filepath.Join(dir, "workspace")
	_ = os.MkdirAll(ws, 0o700)
	g, err := workspace.NewGuard(ws)
	if err != nil {
		t.Fatal(err)
	}
	rec, err := provenance.NewRecorder(dir, "s1", "test-model")
	if err != nil {
		t.Fatal(err)
	}
	var got []event.Event
	sink := wrapProvenanceSink(event.FuncSink(func(e event.Event) { got = append(got, e) }), rec, g)
	sink.Emit(event.Event{
		Kind: event.ToolDispatch,
		Tool: event.Tool{ID: "c1", Name: "write_file", Args: `{"path":"notes.md","content":"hi"}`},
	})
	_ = os.WriteFile(filepath.Join(ws, "notes.md"), []byte("hi"), 0o600)
	sink.Emit(event.Event{
		Kind: event.ToolResult,
		Tool: event.Tool{ID: "c1", Name: "write_file", Output: "wrote 2 bytes to notes.md"},
	})
	if len(got) != 2 {
		t.Fatalf("events = %d", len(got))
	}
	data, err := os.ReadFile(filepath.Join(dir, "provenance.jsonl"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), "workspace/notes.md") {
		t.Fatalf("provenance missing write: %s", data)
	}
}
