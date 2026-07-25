// Package pipeline runs offline science productivity loops with SHA-256 artifacts.
package pipeline

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/lumen-ai/lumen-science/mcp/artifacts"
	"github.com/lumen-ai/lumen-science/standalone/internal/seqbench"
)

// Result is the offline pipeline product.
type Result struct {
	ProjectID       string                 `json:"project_id"`
	RunID           string                 `json:"run_id"`
	SourceArtifact  *artifacts.ArtifactMeta `json:"source_artifact"`
	AnalysisArtifact *artifacts.ArtifactMeta `json:"analysis_artifact"`
	ReportArtifact  *artifacts.ArtifactMeta `json:"report_artifact"`
	Review          ReviewVerdict          `json:"review"`
	Analysis        seqbench.Analysis      `json:"analysis"`
	StoreRoot       string                 `json:"store_root"`
}

// ReviewVerdict is a minimal integrity review (not scientific truth).
type ReviewVerdict struct {
	Status            string   `json:"status"` // pass | fail
	SourceSHA256      string   `json:"source_sha256"`
	AnalysisSHA256    string   `json:"analysis_sha256"`
	ReportSHA256      string   `json:"report_sha256"`
	Checks            []string `json:"checks"`
	Failed            []string `json:"failed,omitempty"`
	ReviewedAt        string   `json:"reviewed_at"`
}

// RunSeqOffline:
//
//	source bytes → artifact register → seqbench → JSON+MD artifacts → integrity review
func RunSeqOffline(storeRoot, projectID, runID, sourceName string, source []byte) (*Result, error) {
	if projectID == "" || runID == "" {
		return nil, fmt.Errorf("project_id and run_id required")
	}
	if len(source) == 0 {
		return nil, fmt.Errorf("empty source")
	}
	if sourceName == "" {
		sourceName = "input.fa"
	}
	store, err := artifacts.NewStoreAt(storeRoot)
	if err != nil {
		return nil, err
	}

	srcMeta, err := store.Write(projectID, runID, "raw/"+filepath.Base(sourceName), "source sequence", "text/x-fasta", source)
	if err != nil {
		return nil, fmt.Errorf("register source: %w", err)
	}

	recs, err := seqbench.ParseFASTA(string(source))
	if err != nil {
		return nil, fmt.Errorf("parse sequence: %w", err)
	}
	analysis := seqbench.Analyze(recs)
	analysisJSON, err := json.MarshalIndent(analysis, "", "  ")
	if err != nil {
		return nil, err
	}
	reportMD := seqbench.MarkdownReport(analysis, sourceName)

	anMeta, err := store.Write(projectID, runID, "derived/analysis.json", "seqbench analysis", "application/json", analysisJSON)
	if err != nil {
		return nil, fmt.Errorf("register analysis: %w", err)
	}
	repMeta, err := store.Write(projectID, runID, "derived/report.md", "seqbench report", "text/markdown", []byte(reportMD))
	if err != nil {
		return nil, fmt.Errorf("register report: %w", err)
	}

	review := reviewIntegrity(store, projectID, runID, srcMeta, anMeta, repMeta)

	return &Result{
		ProjectID:        projectID,
		RunID:            runID,
		SourceArtifact:   srcMeta,
		AnalysisArtifact: anMeta,
		ReportArtifact:   repMeta,
		Review:           review,
		Analysis:         analysis,
		StoreRoot:        storeRoot,
	}, nil
}

func reviewIntegrity(store *artifacts.Store, projectID, runID string, metas ...*artifacts.ArtifactMeta) ReviewVerdict {
	v := ReviewVerdict{
		Status:     "pass",
		ReviewedAt: time.Now().UTC().Format(time.RFC3339),
		Checks:     []string{"artifact_registered", "sha256_rehash", "path_no_traversal"},
	}
	if len(metas) >= 1 {
		v.SourceSHA256 = metas[0].SHA256
	}
	if len(metas) >= 2 {
		v.AnalysisSHA256 = metas[1].SHA256
	}
	if len(metas) >= 3 {
		v.ReportSHA256 = metas[2].SHA256
	}
	for _, m := range metas {
		if m == nil {
			v.Status = "fail"
			v.Failed = append(v.Failed, "nil_meta")
			continue
		}
		data, meta, err := store.Read(projectID, runID, m.Path)
		if err != nil {
			v.Status = "fail"
			v.Failed = append(v.Failed, fmt.Sprintf("read %s: %v", m.Path, err))
			continue
		}
		sum := fmt.Sprintf("%x", sha256.Sum256(data))
		if sum != m.SHA256 || (meta != nil && meta.SHA256 != m.SHA256) {
			v.Status = "fail"
			v.Failed = append(v.Failed, fmt.Sprintf("hash_mismatch %s", m.Path))
		}
	}
	// Persist review
	body, _ := json.MarshalIndent(v, "", "  ")
	_, _ = store.Write(projectID, runID, "review/integrity.json", "integrity review", "application/json", body)
	return v
}

// DefaultStoreRoot returns ~/.lumen/science/artifacts
func DefaultStoreRoot() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, ".lumen", "science", "artifacts"), nil
}
