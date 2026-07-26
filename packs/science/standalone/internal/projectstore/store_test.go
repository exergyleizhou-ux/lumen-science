package projectstore

import (
	"os"
	"path/filepath"
	"testing"
)

func TestCreateListClaim(t *testing.T) {
	dir := t.TempDir()
	s := New(dir)
	p, err := s.Create("owner", "title", "question?")
	if err != nil {
		t.Fatal(err)
	}
	if p.Status != "Draft" {
		t.Fatalf("status %s", p.Status)
	}
	list, err := s.List()
	if err != nil || len(list) != 1 {
		t.Fatalf("list: %v %d", err, len(list))
	}
	c, err := s.ProposeClaim(p.ProjectID, "owner", "a claim", "sci")
	if err != nil {
		t.Fatal(err)
	}
	if c.Status != "Proposed" || c.EvidenceNodeID == nil {
		t.Fatalf("claim: %+v", c)
	}
	// durable files
	if _, err := os.Stat(filepath.Join(dir, "projects", p.ProjectID, "project.json")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dir, "projects", p.ProjectID, "claims", c.ClaimID+".json")); err != nil {
		t.Fatal(err)
	}
	// ownership
	if _, err := s.ProposeClaim(p.ProjectID, "other", "x", "x"); err == nil {
		t.Fatal("expected ownership error")
	}
}

func TestWorkflowValidateRejectsShell(t *testing.T) {
	s := New(t.TempDir())
	spec := map[string]any{
		"workflow_id": "wf-shell",
		"steps": []any{
			map[string]any{"step_id": "bad", "kind": "Shell", "inputs": []any{}},
		},
	}
	val, err := s.WorkflowValidate(spec)
	if err != nil {
		t.Fatal(err)
	}
	if val["is_valid"].(bool) {
		t.Fatalf("Shell step must be rejected, got valid: %+v", val)
	}
	if val["has_unknown_step"].(bool) != true {
		t.Fatal("has_unknown_step must be true")
	}
}

func TestWorkflowValidateAcceptsGoodDAG(t *testing.T) {
	s := New(t.TempDir())
	spec := map[string]any{
		"workflow_id": "wf-ok",
		"steps": []any{
			map[string]any{"step_id": "pubmed", "kind": "ConnectorFetch", "inputs": []any{}},
			map[string]any{"step_id": "review", "kind": "Reviewer", "inputs": []any{"pubmed"}},
		},
	}
	val, err := s.WorkflowValidate(spec)
	if err != nil {
		t.Fatal(err)
	}
	if !val["is_valid"].(bool) {
		t.Fatalf("good DAG must be valid, got: %+v", val)
	}
}

func TestWorkflowDryRunReturnsPrimaryFields(t *testing.T) {
	s := New(t.TempDir())
	spec := map[string]any{
		"workflow_id": "wf-dry",
		"steps": []any{
			map[string]any{"step_id": "fetch", "kind": "ConnectorFetch", "inputs": []any{}},
		},
	}
	dr, err := s.WorkflowDryRun(spec)
	if err != nil {
		t.Fatal(err)
	}
	if dr["can_run"] != true {
		t.Fatalf("can_run: %+v", dr)
	}
	allowed := dr["allowed_steps"].([]string)
	if len(allowed) != 1 {
		t.Fatalf("allowed_steps: %+v", dr)
	}
	rej := dr["rejected_unknown"].([]string)
	if len(rej) != 0 {
		t.Fatalf("rejected_unknown must be empty for good DAG: %+v", dr)
	}
}

func TestWorkflowDryRunRejectsShell(t *testing.T) {
	s := New(t.TempDir())
	spec := map[string]any{
		"workflow_id": "wf-shell2",
		"steps": []any{
			map[string]any{"step_id": "impossible", "kind": "Shell", "inputs": []any{}},
		},
	}
	dr, err := s.WorkflowDryRun(spec)
	if err != nil {
		t.Fatal(err)
	}
	if dr["can_run"] != false {
		t.Fatalf("Shell dry-run must not say can_run: %+v", dr)
	}
	rej := dr["rejected_unknown"].([]string)
	if len(rej) == 0 {
		t.Fatal("Shell step must appear in rejected_unknown")
	}
}

func TestMigrateFromV1ReturnsResultFields(t *testing.T) {
	s := New(t.TempDir())
	res, err := s.MigrateFromV1("run-1", "owner", "title", "question?")
	if err != nil {
		t.Fatal(err)
	}
	for _, k := range []string{"source_run_id", "target_project_id", "artifacts_migrated", "evidence_items_migrated", "hash_verification"} {
		if _, ok := res[k]; !ok {
			t.Fatalf("missing key %s in %+v", k, res)
		}
	}
	if res["hash_verification"] != "Verified" {
		t.Fatalf("hash_verification: %v", res["hash_verification"])
	}
}
