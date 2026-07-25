package artifacts

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
	"unicode"
)

// ArtifactMeta holds metadata stored alongside each artifact.
type ArtifactMeta struct {
	ArtifactID string `json:"artifact_id"`
	Path       string `json:"path"`
	Label      string `json:"label"`
	MimeType   string `json:"mime_type"`
	Bytes      int64  `json:"bytes"`
	SHA256     string `json:"sha256"`
	CreatedAt  string `json:"created_at"`
}

// ArtifactPreview summarises an artifact's contents for display.
type ArtifactPreview struct {
	MimeType    string   `json:"mime_type"`
	PreviewType string   `json:"preview_type"`
	Size        int64    `json:"size"`
	Columns     []string `json:"columns,omitempty"`
	RowCount    int      `json:"row_count,omitempty"`
	SeqCount    int      `json:"seq_count,omitempty"`
}

// Store persists artifacts on the filesystem.
type Store struct {
	root string
}

// NewStore creates a store rooted at ~/.lumen/science/artifacts.
func NewStore() (*Store, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return nil, fmt.Errorf("home dir: %w", err)
	}
	return NewStoreAt(filepath.Join(home, ".lumen", "science", "artifacts"))
}

// NewStoreAt creates a store at an explicit root (useful for testing).
func NewStoreAt(root string) (*Store, error) {
	if err := os.MkdirAll(root, 0o755); err != nil {
		return nil, fmt.Errorf("mkdir: %w", err)
	}
	return &Store{root: root}, nil
}

func (s *Store) dir(projectID, runID string) string {
	return filepath.Join(s.root, projectID, runID)
}

func sanitizePath(p string) error {
	if p == "" || filepath.IsAbs(p) || strings.Contains(p, "..") {
		return fmt.Errorf("invalid path: %q", p)
	}
	return nil
}

// Write persists content and returns metadata.
func (s *Store) Write(projectID, runID, relPath, label, mimeType string, content []byte) (*ArtifactMeta, error) {
	if err := sanitizePath(relPath); err != nil {
		return nil, err
	}
	d := s.dir(projectID, runID)
	if err := os.MkdirAll(d, 0o755); err != nil {
		return nil, err
	}
	h := sha256.Sum256(content)
	sha := fmt.Sprintf("%x", h)
	id := sha[:16]
	meta := ArtifactMeta{
		ArtifactID: id,
		Path:       relPath,
		Label:      label,
		MimeType:   mimeType,
		Bytes:      int64(len(content)),
		SHA256:     sha,
		CreatedAt:  time.Now().UTC().Format(time.RFC3339),
	}
	// atomic write: temp file + rename
	tmp, err := os.CreateTemp(d, ".artifact-*.tmp")
	if err != nil {
		return nil, err
	}
	tmpPath := tmp.Name()
	defer os.Remove(tmpPath)
	if _, err := tmp.Write(content); err != nil {
		tmp.Close()
		return nil, err
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return nil, err
	}
	if err := tmp.Close(); err != nil {
		return nil, err
	}
	target := filepath.Join(d, relPath)
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		return nil, err
	}
	if err := os.Rename(tmpPath, target); err != nil {
		return nil, err
	}
	// write meta
	metaBytes, _ := json.MarshalIndent(meta, "", "  ")
	metaPath := target + ".meta.json"
	if err := os.WriteFile(metaPath, metaBytes, 0o644); err != nil {
		return nil, err
	}
	return &meta, nil
}

// List returns metadata for every artifact under projectID/runID.
func (s *Store) List(projectID, runID string) ([]ArtifactMeta, error) {
	d := s.dir(projectID, runID)
	entries, err := os.ReadDir(d)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var metas []ArtifactMeta
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".meta.json") {
			continue
		}
		data, err := os.ReadFile(filepath.Join(d, e.Name()))
		if err != nil {
			continue
		}
		var m ArtifactMeta
		if json.Unmarshal(data, &m) == nil {
			metas = append(metas, m)
		}
	}
	return metas, nil
}

// Read returns raw content and optional metadata for an artifact.
func (s *Store) Read(projectID, runID, relPath string) ([]byte, *ArtifactMeta, error) {
	if err := sanitizePath(relPath); err != nil {
		return nil, nil, err
	}
	target := filepath.Join(s.dir(projectID, runID), relPath)
	data, err := os.ReadFile(target)
	if err != nil {
		return nil, nil, err
	}
	metaBytes, err := os.ReadFile(target + ".meta.json")
	if err != nil {
		return data, nil, nil
	}
	var m ArtifactMeta
	json.Unmarshal(metaBytes, &m)
	return data, &m, nil
}

// Preview returns a content-aware summary of an artifact.
func (s *Store) Preview(projectID, runID, relPath string) (*ArtifactPreview, error) {
	data, _, err := s.Read(projectID, runID, relPath)
	if err != nil {
		return nil, err
	}
	return sniff(data), nil
}

// --- sniff helpers -----------------------------------------------------------

func sniff(data []byte) *ArtifactPreview {
	preview := &ArtifactPreview{Size: int64(len(data))}
	sample := data
	if len(sample) > 8192 {
		sample = sample[:8192]
	}
	if looksLikeJSON(sample) {
		preview.PreviewType = "json"
		preview.MimeType = "application/json"
	} else if looksLikeFASTA(sample) {
		preview.PreviewType = "fasta"
		preview.MimeType = "text/x-fasta"
		preview.SeqCount = fastaSeqCount(data)
	} else if looksLikeCSV(sample) {
		preview.PreviewType = "csv"
		preview.MimeType = "text/csv"
		preview.Columns, preview.RowCount = csvPreview(data)
	} else {
		preview.PreviewType = "binary"
		preview.MimeType = "application/octet-stream"
	}
	return preview
}

func looksLikeCSV(data []byte) bool {
	if len(data) == 0 {
		return false
	}
	// count commas vs tabs in first line
	first := firstLine(data)
	commas := strings.Count(first, ",")
	tabs := strings.Count(first, "\t")
	return (commas >= 1 || tabs >= 1) && len(first) > 0
}

func csvPreview(data []byte) ([]string, int) {
	lines := strings.Split(string(data), "\n")
	rowCount := 0
	// count non-empty lines; last line may be empty from trailing newline
	for _, l := range lines {
		if strings.TrimSpace(l) != "" {
			rowCount++
		}
	}
	var cols []string
	if len(lines) > 0 && strings.TrimSpace(lines[0]) != "" {
		sep := ","
		if strings.Count(lines[0], "\t") > strings.Count(lines[0], ",") {
			sep = "\t"
		}
		for _, c := range strings.Split(lines[0], sep) {
			cols = append(cols, strings.TrimSpace(c))
		}
	}
	// row count excludes the header line
	if rowCount > 0 {
		rowCount--
	}
	return cols, rowCount
}

func looksLikeFASTA(data []byte) bool {
	if len(data) == 0 {
		return false
	}
	// a FASTA file usually starts with '>'
	return data[0] == '>'
}

func fastaSeqCount(data []byte) int {
	n := 0
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, ">") {
			n++
		}
	}
	return n
}

func looksLikeJSON(data []byte) bool {
	if len(data) == 0 {
		return false
	}
	// skip leading whitespace
	s := strings.TrimLeftFunc(string(data), unicode.IsSpace)
	return strings.HasPrefix(s, "{") || strings.HasPrefix(s, "[")
}

func firstLine(data []byte) string {
	s := string(data)
	if idx := strings.IndexByte(s, '\n'); idx >= 0 {
		return s[:idx]
	}
	return s
}
