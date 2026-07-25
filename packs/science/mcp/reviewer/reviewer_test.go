package reviewer

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/lumen-ai/lumen-science/mcp"
)

func reviewerTools(t *testing.T, dir string) ([]mcp.RegisteredTool, *Reviewer) {
	t.Helper()
	r := NewReviewer(dir)
	srv := &Server{Reviewer: r}
	return Tools(srv), r
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

// setupArtifact creates a realistic artifact file with correct meta.json in a temp dir.
func setupArtifact(t *testing.T, root, projectID, runID, path, content string) {
	t.Helper()
	dir := filepath.Join(root, projectID, runID)
	if err := os.MkdirAll(filepath.Join(dir, filepath.Dir(path)), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, path), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	h := sha256.Sum256([]byte(content))
	sha := fmt.Sprintf("%x", h)
	meta := map[string]any{
		"artifact_id": sha[:16],
		"path":        path,
		"label":       path,
		"mime_type":   "text/plain",
		"bytes":       len(content),
		"sha256":      sha,
		"created_at":  "2025-01-01T00:00:00Z",
	}
	metaBytes, _ := json.MarshalIndent(meta, "", "  ")
	if err := os.WriteFile(filepath.Join(dir, path+".meta.json"), metaBytes, 0o644); err != nil {
		t.Fatal(err)
	}
}

// TestValidReview verifies that a well-formed artifact set passes review.
func TestValidReview(t *testing.T) {
	dir := t.TempDir()
	tools, _ := reviewerTools(t, dir)

	setupArtifact(t, dir, "proj", "run1", "data.csv", "a,b,c\n1,2,3")
	setupArtifact(t, dir, "proj", "run1", "genes.fasta", ">seq1\nACGT")

	handler := findHandler(t, tools, "start_review")
	args, _ := json.Marshal(map[string]string{
		"project_id": "proj",
		"run_id":     "run1",
	})
	result, err := handler(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	txt := c["text"].(string)

	var report ReviewReport
	if err := json.Unmarshal([]byte(txt), &report); err != nil {
		t.Fatalf("unmarshal report: %v (raw: %s)", err, txt)
	}
	if report.Status != StatusPass {
		t.Fatalf("expected pass, got %s: %v", report.Status, report.Checks)
	}
	if len(report.Checks) != 1 {
		t.Fatalf("expected 1 check, got %d", len(report.Checks))
	}
	if report.Checks[0].Name != "artifact_integrity" {
		t.Fatalf("expected artifact_integrity check, got %s", report.Checks[0].Name)
	}
	if report.Checks[0].Status != "pass" {
		t.Fatalf("expected pass, got %s: %s", report.Checks[0].Status, report.Checks[0].Message)
	}
}

// TestEmptyDirectory verifies review of a non-existent run returns an error.
func TestEmptyDirectory(t *testing.T) {
	dir := t.TempDir()
	tools, _ := reviewerTools(t, dir)

	handler := findHandler(t, tools, "start_review")
	args, _ := json.Marshal(map[string]string{
		"project_id": "nonexistent",
		"run_id":     "norun",
	})
	result, err := handler(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	txt := c["text"].(string)
	if !strings.Contains(txt, "run directory not found") {
		t.Fatalf("expected 'run directory not found' error, got: %s", txt)
	}
}

// TestEmptyArtifacts verifies that an existing directory with no artifacts fails.
func TestEmptyArtifacts(t *testing.T) {
	dir := t.TempDir()
	tools, _ := reviewerTools(t, dir)

	// Create the run directory with no artifact files.
	runDir := filepath.Join(dir, "proj", "empty")
	if err := os.MkdirAll(runDir, 0o755); err != nil {
		t.Fatal(err)
	}

	handler := findHandler(t, tools, "start_review")
	args, _ := json.Marshal(map[string]string{
		"project_id": "proj",
		"run_id":     "empty",
	})
	result, err := handler(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	txt := c["text"].(string)

	var report ReviewReport
	if err := json.Unmarshal([]byte(txt), &report); err != nil {
		t.Fatalf("unmarshal report: %v", err)
	}
	if report.Status != StatusFail {
		t.Fatalf("expected fail for empty dir, got %s", report.Status)
	}
	if len(report.Checks) != 1 {
		t.Fatalf("expected 1 check, got %d", len(report.Checks))
	}
	if report.Checks[0].Status != "fail" {
		t.Fatalf("expected fail check, got %s", report.Checks[0].Status)
	}
}

// TestHashMismatch verifies detection of a corrupted artifact.
func TestHashMismatch(t *testing.T) {
	dir := t.TempDir()
	tools, _ := reviewerTools(t, dir)

	setupArtifact(t, dir, "proj", "run1", "data.csv", "a,b,c\n1,2,3")

	// Corrupt the meta.json with a wrong hash.
	metaPath := filepath.Join(dir, "proj", "run1", "data.csv.meta.json")
	metaData, err := os.ReadFile(metaPath)
	if err != nil {
		t.Fatal(err)
	}
	var meta map[string]any
	if err := json.Unmarshal(metaData, &meta); err != nil {
		t.Fatal(err)
	}
	meta["sha256"] = "0000000000000000000000000000000000000000000000000000000000000000"
	corrupt, _ := json.MarshalIndent(meta, "", "  ")
	if err := os.WriteFile(metaPath, corrupt, 0o644); err != nil {
		t.Fatal(err)
	}

	handler := findHandler(t, tools, "start_review")
	args, _ := json.Marshal(map[string]string{
		"project_id": "proj",
		"run_id":     "run1",
	})
	result, err := handler(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	txt := c["text"].(string)

	var report ReviewReport
	if err := json.Unmarshal([]byte(txt), &report); err != nil {
		t.Fatalf("unmarshal report: %v", err)
	}
	if report.Status != StatusFail {
		t.Fatalf("expected fail, got %s", report.Status)
	}
	if len(report.Checks) != 1 {
		t.Fatalf("expected 1 check, got %d", len(report.Checks))
	}
	if report.Checks[0].Status != "fail" {
		t.Fatalf("expected fail check, got %s", report.Checks[0].Status)
	}
	if !strings.Contains(report.Checks[0].Message, "invalid or missing") {
		t.Fatalf("expected 'invalid or missing' message, got: %s", report.Checks[0].Message)
	}
}

// TestAllThreeTools verifies start_review, review_status, and approve_fix all exist.
func TestAllThreeTools(t *testing.T) {
	dir := t.TempDir()
	r := NewReviewer(dir)
	srv := &Server{Reviewer: r}
	tools := Tools(srv)

	names := make(map[string]bool)
	for _, reg := range tools {
		names[reg.Tool.Name] = true
	}
	required := []string{"start_review", "review_status", "approve_fix"}
	for _, name := range required {
		if !names[name] {
			t.Fatalf("missing tool %q", name)
		}
	}
	if len(tools) != 3 {
		t.Fatalf("expected 3 tools, got %d", len(tools))
	}
}

// TestApproveFixAfterHashMismatch verifies that approve_fix passes after the artifact is corrected.
func TestApproveFixAfterHashMismatch(t *testing.T) {
	dir := t.TempDir()
	tools, _ := reviewerTools(t, dir)

	setupArtifact(t, dir, "proj", "run1", "data.csv", "a,b,c\n1,2,3")

	// Initially corrupt the hash.
	metaPath := filepath.Join(dir, "proj", "run1", "data.csv.meta.json")
	metaData, err := os.ReadFile(metaPath)
	if err != nil {
		t.Fatal(err)
	}
	var meta map[string]any
	if err := json.Unmarshal(metaData, &meta); err != nil {
		t.Fatal(err)
	}
	meta["sha256"] = "0000000000000000000000000000000000000000000000000000000000000000"
	corrupt, _ := json.MarshalIndent(meta, "", "  ")
	if err := os.WriteFile(metaPath, corrupt, 0o644); err != nil {
		t.Fatal(err)
	}

	// First review should fail.
	handler := findHandler(t, tools, "start_review")
	args, _ := json.Marshal(map[string]string{
		"project_id": "proj",
		"run_id":     "run1",
	})
	result, err := handler(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	m := result.(map[string]any)
	c := m["content"].([]any)[0].(map[string]any)
	var report ReviewReport
	json.Unmarshal([]byte(c["text"].(string)), &report)
	if report.Status != StatusFail {
		t.Fatalf("initial review should fail, got %s", report.Status)
	}

	// Fix the artifact by rewriting with correct hash.
	setupArtifact(t, dir, "proj", "run1", "data.csv", "a,b,c\n1,2,3")

	// approve_fix should pass.
	fixHandler := findHandler(t, tools, "approve_fix")
	result2, err := fixHandler(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	m2 := result2.(map[string]any)
	c2 := m2["content"].([]any)[0].(map[string]any)
	var report2 ReviewReport
	json.Unmarshal([]byte(c2["text"].(string)), &report2)
	if report2.Status != StatusPass {
		t.Fatalf("approve_fix should pass after fix, got %s: %v", report2.Status, report2.Checks)
	}
}
