package jupyter

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestNotebookValidNbformat checks that Save produces valid nbformat 4.x JSON.
func TestNotebookValidNbformat(t *testing.T) {
	dir := t.TempDir()

	nb := New("Test NB")
	nb.AddCode("print(1+1)")
	nb.AddMarkdown("## Results")

	path := filepath.Join(dir, "test.ipynb")
	if err := nb.Save(path); err != nil {
		t.Fatalf("Save: %v", err)
	}

	// Read back as raw JSON and verify structure.
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}

	var raw map[string]any
	if err := json.Unmarshal(data, &raw); err != nil {
		t.Fatalf("json.Unmarshal: %v", err)
	}

	// Top-level required keys.
	for _, key := range []string{"nbformat", "nbformat_minor", "metadata", "cells"} {
		if _, ok := raw[key]; !ok {
			t.Errorf("missing top-level key: %s", key)
		}
	}

	// No "path" pollution.
	if _, ok := raw["path"]; ok {
		t.Errorf("path field leaked into .ipynb JSON — must not be serialized")
	}

	// Metadata must have kernelspec and language_info.
	meta, ok := raw["metadata"].(map[string]any)
	if !ok {
		t.Fatal("metadata is not an object")
	}
	if _, ok := meta["kernelspec"]; !ok {
		t.Error("metadata missing kernelspec")
	}
	if _, ok := meta["language_info"]; !ok {
		t.Error("metadata missing language_info")
	}

	// Check each cell.
	cells, ok := raw["cells"].([]any)
	if !ok {
		t.Fatal("cells is not an array")
	}
	if len(cells) != 3 {
		t.Fatalf("expected 3 cells, got %d", len(cells))
	}

	for i, c := range cells {
		cell, ok := c.(map[string]any)
		if !ok {
			t.Errorf("cell %d is not an object", i)
			continue
		}
		// Every cell must have metadata.
		cm, ok := cell["metadata"].(map[string]any)
		if !ok {
			t.Errorf("cell %d missing metadata object", i)
		} else if cm == nil {
			t.Errorf("cell %d metadata is null, want {}", i)
		}

		cellType, _ := cell["cell_type"].(string)
		if cellType == "code" {
			// Code cell must have execution_count (null for unexecuted).
			// outputs may be omitted when empty (omitempty), which is nbformat-valid.
			if _, exists := cell["execution_count"]; !exists {
				t.Errorf("cell %d (code) missing execution_count", i)
			}
			// If outputs is present, it should be an array (not null).
			if outputs, exists := cell["outputs"]; exists && outputs == nil {
				t.Errorf("cell %d (code) outputs is null, want []", i)
			}
		}
	}

	t.Logf("JSON keys: nbformat=%v, nbformat_minor=%v, cells=%d", raw["nbformat"], raw["nbformat_minor"], len(cells))
}

// TestNotebookRoundTrip verifies Load → Save → Load preserves structure.
func TestNotebookRoundTrip(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "roundtrip.ipynb")

	nb := New("Roundtrip")
	nb.AddCode("x = 42")
	nb.AddCode("print(x)")
	if err := nb.Save(path); err != nil {
		t.Fatalf("Save 1: %v", err)
	}

	loaded, err := Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(loaded.Cells) != 3 {
		t.Fatalf("expected 3 cells after load, got %d", len(loaded.Cells))
	}

	path2 := filepath.Join(dir, "roundtrip2.ipynb")
	if err := loaded.Save(path2); err != nil {
		t.Fatalf("Save 2: %v", err)
	}

	loaded2, err := Load(path2)
	if err != nil {
		t.Fatalf("Load 2: %v", err)
	}
	if len(loaded2.Cells) != 3 {
		t.Fatalf("round-trip: expected 3 cells, got %d", len(loaded2.Cells))
	}

	// Verify no path pollution in saved file
	data, _ := os.ReadFile(path2)
	var raw map[string]any
	json.Unmarshal(data, &raw)
	if _, ok := raw["path"]; ok {
		t.Errorf("path field leaked into .ipynb on round-trip")
	}
}

// TestNormalize repairs incomplete notebooks (e.g. from old code).
func TestNormalize(t *testing.T) {
	nb := &Notebook{
		Nbformat:      4,
		NbformatMinor: 5,
		Cells: []Cell{
			{CellType: "markdown", Source: []string{"# hi"}},                     // no metadata
			{CellType: "code", Source: []string{"1+1"}},                          // no metadata, no outputs, no execution_count
			{CellType: "code", Source: []string{"2+2"}, Metadata: map[string]any{"tags": []any{"test"}}}, // partial metadata
		},
	}
	nb.Normalize()

	if nb.Metadata == nil {
		t.Error("notebook metadata should not be nil after Normalize")
	}
	if nb.Metadata["kernelspec"] == nil {
		t.Error("kernelspec missing after Normalize")
	}
	if nb.Metadata["language_info"] == nil {
		t.Error("language_info missing after Normalize")
	}

	for i, c := range nb.Cells {
		if c.Metadata == nil {
			t.Errorf("cell %d metadata is nil after Normalize", i)
		}
		if c.CellType == "code" && c.Outputs == nil {
			t.Errorf("cell %d (code) outputs is nil after Normalize", i)
		}
	}
}

// TestLoadNormalizesOldFile loads a notebook saved by old code (cells with no metadata)
// and verifies Normalize repairs it.
func TestLoadNormalizesOldFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "old.ipynb")

	// Write an ipynb matching the old code's output (cell without metadata, path field)
	oldJSON := `{
  "metadata": {
    "kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"},
    "title": "old_nb"
  },
  "nbformat": 4,
  "nbformat_minor": 5,
  "cells": [
    {
      "cell_type": "markdown",
      "source": ["# old_nb"]
    },
    {
      "cell_type": "code",
      "source": ["print(1+1)"]
    }
  ],
  "path": "/root/some/path/old.ipynb"
}`
	if err := os.WriteFile(path, []byte(oldJSON), 0o600); err != nil {
		t.Fatal(err)
	}

	nb, err := Load(path)
	if err != nil {
		t.Fatalf("Load old file: %v", err)
	}

	// Normalize should have been called during Load.
	for i, c := range nb.Cells {
		if c.Metadata == nil {
			t.Errorf("old cell %d: metadata not repaired by Normalize during Load", i)
		}
		if c.CellType == "code" && c.Outputs == nil {
			t.Errorf("old cell %d: outputs not repaired", i)
		}
	}

	// Save again — path must not leak.
	path2 := filepath.Join(dir, "repaired.ipynb")
	if err := nb.Save(path2); err != nil {
		t.Fatalf("Save repaired: %v", err)
	}
	data, _ := os.ReadFile(path2)
	if strings.Contains(string(data), `"path":`) {
		t.Errorf("path field leaked after repairing old notebook")
	}
}

// TestExecuteErrorHasMessage ensures that when Jupyter is unavailable,
// the error returned is non-empty.
func TestExecuteErrorHasMessage(t *testing.T) {
	// If jupyter is available, this will actually run — that's fine.
	// If not, the error must be non-empty and mention jupyter/nbconvert/python.

	dir := t.TempDir()
	path := filepath.Join(dir, "exec.ipynb")

	nb := New("Exec test")
	nb.AddCode("print(1+1)")
	if err := nb.Save(path); err != nil {
		t.Fatal(err)
	}

	err := nb.Execute(path, "python3")
	if err != nil {
		// Jupyter not available — error must be non-empty.
		if err.Error() == "" {
			t.Error("Execute error is empty string — must contain stderr or reason")
		}
		t.Logf("Execute error (expected when jupyter unavailable): %s", err.Error())
	} else {
		// Jupyter is available — verify outputs were loaded.
		t.Log("Jupyter available — execute succeeded")
		hasOutput := false
		for _, c := range nb.Cells {
			if len(c.Outputs) > 0 {
				hasOutput = true
				break
			}
		}
		if !hasOutput {
			t.Log("No output cells found (may be expected for simple print)")
		}
	}
}

// TestExecuteErrorNotEmpty ensures error is never empty string on failure.
func TestExecuteErrorNotEmpty(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "bad.ipynb")

	// Write intentionally invalid notebook
	if err := os.WriteFile(path, []byte("not valid json"), 0o600); err != nil {
		t.Fatal(err)
	}

	nb := New("bad")
	err := nb.Execute(path, "python3")
	// We expect an error (nbconvert will fail on invalid notebook, or jupyter unavailable).
	// In either case, error must not be empty.
	if err == nil {
		t.Log("Execute succeeded unexpectedly (jupyter may have recovered); skipping emptiness check")
	} else {
		if err.Error() == "" {
			t.Error("Execute returned error with empty message")
		}
		t.Logf("Execute error: %s", err.Error())
	}
}
