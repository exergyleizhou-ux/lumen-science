package reviewer

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// ReviewStatus classifies the overall result of a review.
type ReviewStatus string

const (
	StatusPass ReviewStatus = "pass"
	StatusWarn ReviewStatus = "warn"
	StatusFail ReviewStatus = "fail"
)

// ReviewReport holds the outcome of a single review run.
type ReviewReport struct {
	RunID     string       `json:"run_id"`
	ProjectID string       `json:"project_id"`
	Status    ReviewStatus `json:"status"`
	Checks    []CheckItem  `json:"checks"`
	Summary   string       `json:"summary"`
	FixCount  int          `json:"fix_count"`
}

// CheckItem describes one pass / warn / fail check.
type CheckItem struct {
	Name    string `json:"name"`
	Status  string `json:"status"`
	Message string `json:"message"`
}

// Reviewer verifies artifacts stored by the Artifacts MCP server.
type Reviewer struct {
	artifactsRoot string
}

// NewReviewer creates a Reviewer that reads from the given artifacts root.
func NewReviewer(artifactsRoot string) *Reviewer {
	return &Reviewer{artifactsRoot: artifactsRoot}
}

// Review runs integrity checks on the artifact set for a given project/run pair.
func (r *Reviewer) Review(projectID, runID string) (*ReviewReport, error) {
	dir := filepath.Join(r.artifactsRoot, projectID, runID)

	var checks []CheckItem

	// Check 1: directory exists and is readable.
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, fmt.Errorf("run directory not found: %s/%s", projectID, runID)
	}

	// Check 2: artifacts exist and have valid SHA-256 metadata.
	artifactCount := 0
	hashVerified := 0
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		if filepath.Ext(e.Name()) == ".json" {
			continue
		}
		artifactCount++
		// Read artifact content.
		data, err := os.ReadFile(filepath.Join(dir, e.Name()))
		if err != nil {
			continue
		}
		// Read metadata.
		metaPath := filepath.Join(dir, e.Name()+".meta.json")
		metaData, err := os.ReadFile(metaPath)
		if err != nil {
			continue
		}
		var meta struct {
			SHA256 string `json:"sha256"`
		}
		if json.Unmarshal(metaData, &meta) != nil {
			continue
		}
		h := sha256.Sum256(data)
		if fmt.Sprintf("%x", h) == meta.SHA256 {
			hashVerified++
		}
	}

	if artifactCount == 0 {
		checks = append(checks, CheckItem{
			Name:    "artifacts_exist",
			Status:  "fail",
			Message: fmt.Sprintf("no artifacts found in %s/%s", projectID, runID),
		})
	} else if hashVerified == artifactCount {
		checks = append(checks, CheckItem{
			Name:    "artifact_integrity",
			Status:  "pass",
			Message: fmt.Sprintf("all %d artifacts have valid SHA-256 hashes", artifactCount),
		})
	} else {
		checks = append(checks, CheckItem{
			Name:    "artifact_integrity",
			Status:  "fail",
			Message: fmt.Sprintf("%d/%d artifacts have invalid or missing SHA-256", artifactCount-hashVerified, artifactCount),
		})
	}

	// Determine overall status.
	hasFail, hasWarn := false, false
	for _, c := range checks {
		if c.Status == "fail" {
			hasFail = true
		}
		if c.Status == "warn" {
			hasWarn = true
		}
	}
	status := StatusPass
	if hasFail {
		status = StatusFail
	} else if hasWarn {
		status = StatusWarn
	}

	return &ReviewReport{
		RunID:     runID,
		ProjectID: projectID,
		Status:    status,
		Checks:    checks,
		Summary:   fmt.Sprintf("Reviewed %d artifacts across %d checks", artifactCount, len(checks)),
	}, nil
}
