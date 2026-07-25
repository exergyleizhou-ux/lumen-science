package artifacts

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"strings"
	"testing"

	"github.com/lumen-ai/lumen-science/mcp"
)

func artifactTools(t *testing.T, dir string) ([]mcp.RegisteredTool, *Store) {
	t.Helper()
	store, err := NewStoreAt(dir)
	if err != nil {
		t.Fatal(err)
	}
	return Tools(store), store
}

func findHandler(t *testing.T, tools []mcp.RegisteredTool, name string) mcp.ToolHandler {
	t.Helper()
	for _, reg := range tools {
		if reg.Tool.Name == name {
			return reg.Handler
		}
	}
	t.Fatalf("tool %q not found", name)
	return nil
}

// TestWriteAndRead exercises artifact_write and artifact_read end-to-end.
func TestWriteAndRead(t *testing.T) {
	dir := t.TempDir()
	tools, _ := artifactTools(t, dir)

	writeHandler := findHandler(t, tools, "artifact_write")

	content := []byte("hello, artifacts")
	writeArgs, _ := json.Marshal(map[string]string{
		"project_id": "proj1",
		"run_id":     "run1",
		"path":       "data/hello.txt",
		"content":    base64.StdEncoding.EncodeToString(content),
		"mime_type":  "text/plain",
		"label":      "hello file",
	})
	result, err := writeHandler(context.Background(), writeArgs)
	if err != nil {
		t.Fatal(err)
	}
	// result should be TextResult wrapping ArtifactMeta
	m, ok := result.(map[string]any)
	if !ok {
		t.Fatalf("expected map, got %T", result)
	}
	contentBlock := m["content"].([]any)
	if len(contentBlock) == 0 {
		t.Fatal("empty content")
	}
	textContent := contentBlock[0].(map[string]any)
	textStr := textContent["text"].(string)
	var meta ArtifactMeta
	if err := json.Unmarshal([]byte(textStr), &meta); err != nil {
		t.Fatalf("unmarshal meta: %v (raw: %s)", err, textStr)
	}
	if meta.ArtifactID == "" {
		t.Fatal("missing artifact_id")
	}
	if meta.Bytes != int64(len(content)) {
		t.Fatalf("bytes: got %d, want %d", meta.Bytes, len(content))
	}
	if meta.MimeType != "text/plain" {
		t.Fatalf("mime_type: got %s", meta.MimeType)
	}

	// Read it back
	readHandler := findHandler(t, tools, "artifact_read")
	readArgs, _ := json.Marshal(map[string]string{
		"project_id": "proj1",
		"run_id":     "run1",
		"path":       "data/hello.txt",
	})
	readResult, err := readHandler(context.Background(), readArgs)
	if err != nil {
		t.Fatal(err)
	}
	rm := readResult.(map[string]any)
	rc := rm["content"].([]any)[0].(map[string]any)
	rt := rc["text"].(string)
	var readPayload struct {
		Content string       `json:"content"`
		Meta    ArtifactMeta `json:"meta"`
	}
	if err := json.Unmarshal([]byte(rt), &readPayload); err != nil {
		t.Fatalf("unmarshal read result: %v", err)
	}
	decoded, err := base64.StdEncoding.DecodeString(readPayload.Content)
	if err != nil {
		t.Fatal(err)
	}
	if string(decoded) != "hello, artifacts" {
		t.Fatalf("content mismatch: %q", string(decoded))
	}
}

// TestList verifies artifact_list returns metadata.
func TestList(t *testing.T) {
	dir := t.TempDir()
	tools, store := artifactTools(t, dir)

	// Write two artifacts
	_, err := store.Write("proj", "run-a", "a.csv", "csv data", "text/csv", []byte("a,b,c\n1,2,3"))
	if err != nil {
		t.Fatal(err)
	}
	_, err = store.Write("proj", "run-a", "b.fasta", "fasta data", "text/x-fasta", []byte(">seq1\nACGT"))
	if err != nil {
		t.Fatal(err)
	}

	listHandler := findHandler(t, tools, "artifact_list")
	listArgs, _ := json.Marshal(map[string]string{
		"project_id": "proj",
		"run_id":     "run-a",
	})
	result, err := listHandler(context.Background(), listArgs)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	txt := c["text"].(string)
	var metas []ArtifactMeta
	if err := json.Unmarshal([]byte(txt), &metas); err != nil {
		t.Fatalf("unmarshal list: %v (raw: %s)", err, txt)
	}
	if len(metas) != 2 {
		t.Fatalf("expected 2 artifacts, got %d", len(metas))
	}
}

// TestPreviewCSV verifies artifact_preview on CSV data.
func TestPreviewCSV(t *testing.T) {
	dir := t.TempDir()
	tools, store := artifactTools(t, dir)

	_, err := store.Write("proj", "run", "data.csv", "csv", "text/csv", []byte("name,age,city\nAlice,30,NYC\nBob,25,LA"))
	if err != nil {
		t.Fatal(err)
	}

	previewHandler := findHandler(t, tools, "artifact_preview")
	previewArgs, _ := json.Marshal(map[string]string{
		"project_id": "proj",
		"run_id":     "run",
		"path":       "data.csv",
	})
	result, err := previewHandler(context.Background(), previewArgs)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	txt := c["text"].(string)
	var preview ArtifactPreview
	if err := json.Unmarshal([]byte(txt), &preview); err != nil {
		t.Fatalf("unmarshal preview: %v", err)
	}
	if preview.PreviewType != "csv" {
		t.Fatalf("expected csv preview, got %s", preview.PreviewType)
	}
	if len(preview.Columns) != 3 {
		t.Fatalf("expected 3 columns, got %v", preview.Columns)
	}
	if preview.RowCount != 2 { // header is counted as a row in csvPreview
		t.Fatalf("expected row count 2, got %d", preview.RowCount)
	}
}

// TestPreviewFASTA verifies artifact_preview on FASTA data.
func TestPreviewFASTA(t *testing.T) {
	dir := t.TempDir()
	tools, store := artifactTools(t, dir)

	fastaData := ">seq1 Human gene\nACGTACGT\n>seq2 Mouse gene\nTGCA\n>seq3 Fish gene\nGGGG"
	_, err := store.Write("proj", "run", "genes.fasta", "fasta", "text/x-fasta", []byte(fastaData))
	if err != nil {
		t.Fatal(err)
	}

	previewHandler := findHandler(t, tools, "artifact_preview")
	previewArgs, _ := json.Marshal(map[string]string{
		"project_id": "proj",
		"run_id":     "run",
		"path":       "genes.fasta",
	})
	result, err := previewHandler(context.Background(), previewArgs)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	txt := c["text"].(string)
	var preview ArtifactPreview
	if err := json.Unmarshal([]byte(txt), &preview); err != nil {
		t.Fatalf("unmarshal preview: %v", err)
	}
	if preview.PreviewType != "fasta" {
		t.Fatalf("expected fasta preview, got %s", preview.PreviewType)
	}
	if preview.SeqCount != 3 {
		t.Fatalf("expected 3 sequences, got %d", preview.SeqCount)
	}
}

// TestPathTraversalRejection verifies that ".." and absolute paths are rejected.
func TestPathTraversalRejection(t *testing.T) {
	dir := t.TempDir()
	tools, _ := artifactTools(t, dir)

	writeHandler := findHandler(t, tools, "artifact_write")

	tests := []string{
		"../etc/passwd",
		"/etc/passwd",
		"",
	}

	for _, p := range tests {
		args, _ := json.Marshal(map[string]string{
			"project_id": "p",
			"run_id":     "r",
			"path":       p,
			"content":    base64.StdEncoding.EncodeToString([]byte("bad")),
			"mime_type":  "text/plain",
		})
		result, err := writeHandler(context.Background(), args)
		if err != nil {
			t.Fatalf("handler returned error for path %q: %v", p, err)
		}
		// the handler returns ErrorResult, which is wrapped in TextResult by dispatch.
		// We check that it's an error response.
		m := result.(map[string]any)
		c := m["content"].([]any)[0].(map[string]any)
		txt := c["text"].(string)
		if !strings.Contains(txt, "invalid path") {
			t.Fatalf("expected 'invalid path' error for %q, got: %s", p, txt)
		}
	}
}

// TestInvalidBase64 verifies reject of bad base64.
func TestInvalidBase64(t *testing.T) {
	dir := t.TempDir()
	tools, _ := artifactTools(t, dir)

	writeHandler := findHandler(t, tools, "artifact_write")
	args, _ := json.Marshal(map[string]string{
		"project_id": "p",
		"run_id":     "r",
		"path":       "test.txt",
		"content":    "!!!not-valid-base64!!!",
		"mime_type":  "text/plain",
	})
	result, err := writeHandler(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	txt := c["text"].(string)
	if !strings.Contains(txt, "invalid base64") {
		t.Fatalf("expected base64 error, got: %s", txt)
	}
}

// TestListEmpty verifies artifact_list returns empty array when no artifacts exist.
func TestListEmpty(t *testing.T) {
	dir := t.TempDir()
	tools, _ := artifactTools(t, dir)

	listHandler := findHandler(t, tools, "artifact_list")
	listArgs, _ := json.Marshal(map[string]string{
		"project_id": "empty-proj",
		"run_id":     "empty-run",
	})
	result, err := listHandler(context.Background(), listArgs)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	txt := c["text"].(string)
	if !strings.Contains(txt, "[]") {
		t.Fatalf("expected empty array, got: %s", txt)
	}
}

// TestPreviewJSON verifies artifact_preview detects JSON.
func TestPreviewJSON(t *testing.T) {
	dir := t.TempDir()
	tools, store := artifactTools(t, dir)

	jsonData := `{"key": "value", "nested": {"a": 1}}`
	_, err := store.Write("proj", "run", "data.json", "json", "application/json", []byte(jsonData))
	if err != nil {
		t.Fatal(err)
	}

	previewHandler := findHandler(t, tools, "artifact_preview")
	previewArgs, _ := json.Marshal(map[string]string{
		"project_id": "proj",
		"run_id":     "run",
		"path":       "data.json",
	})
	result, err := previewHandler(context.Background(), previewArgs)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	txt := c["text"].(string)
	var preview ArtifactPreview
	if err := json.Unmarshal([]byte(txt), &preview); err != nil {
		t.Fatalf("unmarshal preview: %v", err)
	}
	if preview.PreviewType != "json" {
		t.Fatalf("expected json preview, got %s", preview.PreviewType)
	}
}

// TestToolsRegistration verifies the server registers all four tools.
func TestToolsRegistration(t *testing.T) {
	dir := t.TempDir()
	store, err := NewStoreAt(dir)
	if err != nil {
		t.Fatal(err)
	}
	tools := Tools(store)
	names := make(map[string]bool)
	for _, reg := range tools {
		names[reg.Tool.Name] = true
	}
	required := []string{"artifact_write", "artifact_list", "artifact_read", "artifact_preview"}
	for _, name := range required {
		if !names[name] {
			t.Fatalf("missing tool %q", name)
		}
	}
	if len(tools) != 4 {
		t.Fatalf("expected 4 tools, got %d", len(tools))
	}
}
