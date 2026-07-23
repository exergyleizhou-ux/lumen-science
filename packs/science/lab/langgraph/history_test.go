package langgraph

import (
	"os"
	"testing"
)

func TestHistoryAppendAndList(t *testing.T) {
	dir := t.TempDir()
	if err := AppendHistory(dir, HistoryEntry{
		ProjectID: "p",
		Prompt:    "hello",
		OK:        true,
		Result:    "world",
		Mode:      "heuristic",
	}); err != nil {
		t.Fatal(err)
	}
	if err := AppendHistory(dir, HistoryEntry{
		ProjectID: "p",
		Prompt:    "second",
		OK:        false,
		Error:     "x",
		Mode:      "llm",
	}); err != nil {
		t.Fatal(err)
	}
	list, err := ListHistory(dir, 10)
	if err != nil {
		t.Fatal(err)
	}
	if len(list) != 2 {
		t.Fatalf("len %d", len(list))
	}
	// newest first
	if list[0].Prompt != "second" {
		t.Fatalf("newest=%q", list[0].Prompt)
	}
	if list[1].Prompt != "hello" {
		t.Fatalf("older=%q", list[1].Prompt)
	}
	if _, err := os.Stat(HistoryPath(dir)); err != nil {
		t.Fatal(err)
	}
}
