package lab

import (
	"os"
	"path/filepath"
	"strings"
	"unicode/utf8"
)

// FileSearchHit is one match under the project workspace.
type FileSearchHit struct {
	Path        string `json:"path"`
	Name        string `json:"name"`
	IsDir       bool   `json:"isDir"`
	PreviewKind string `json:"previewKind"`
	Match       string `json:"match"` // "name" | "content"
	Snippet     string `json:"snippet,omitempty"`
	Size        int64  `json:"size,omitempty"`
}

// SearchWorkspace finds files by name or text content under root (must already be workspace-guarded abs path).
func SearchWorkspace(root, query string, limit int) ([]FileSearchHit, error) {
	q := strings.TrimSpace(strings.ToLower(query))
	if q == "" {
		return nil, nil
	}
	if limit <= 0 {
		limit = 50
	}
	if limit > 200 {
		limit = 200
	}
	var hits []FileSearchHit
	err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil
		}
		if len(hits) >= limit {
			return filepath.SkipAll
		}
		// skip hidden / heavy dirs
		base := info.Name()
		if base == ".git" || base == "node_modules" || base == ".lumen" {
			if info.IsDir() {
				return filepath.SkipDir
			}
		}
		rel, err := filepath.Rel(root, path)
		if err != nil || rel == "." {
			return nil
		}
		rel = filepath.ToSlash(rel)
		nameLower := strings.ToLower(base)
		if strings.Contains(nameLower, q) || strings.Contains(strings.ToLower(rel), q) {
			hit := FileSearchHit{
				Path:        rel,
				Name:        base,
				IsDir:       info.IsDir(),
				PreviewKind: previewKind(base),
				Match:       "name",
			}
			if !info.IsDir() {
				hit.Size = info.Size()
			}
			hits = append(hits, hit)
			return nil
		}
		if info.IsDir() || info.Size() > 256*1024 {
			return nil
		}
		// content search only for text-ish kinds
		pk := previewKind(base)
		if pk != "text" && pk != "markdown" {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil || !utf8.Valid(data) {
			return nil
		}
		// skip binary-ish
		if strings.Contains(string(data[:min(len(data), 512)]), "\x00") {
			return nil
		}
		lower := strings.ToLower(string(data))
		idx := strings.Index(lower, q)
		if idx < 0 {
			return nil
		}
		snippet := snippetAround(string(data), idx, len(q), 80)
		hits = append(hits, FileSearchHit{
			Path:        rel,
			Name:        base,
			IsDir:       false,
			PreviewKind: pk,
			Match:       "content",
			Snippet:     snippet,
			Size:        info.Size(),
		})
		return nil
	})
	if err == filepath.SkipAll {
		err = nil
	}
	return hits, err
}

func snippetAround(s string, idx, qlen, radius int) string {
	start := idx - radius
	if start < 0 {
		start = 0
	}
	end := idx + qlen + radius
	if end > len(s) {
		end = len(s)
	}
	snip := s[start:end]
	snip = strings.ReplaceAll(snip, "\n", " ")
	if start > 0 {
		snip = "…" + snip
	}
	if end < len(s) {
		snip = snip + "…"
	}
	return snip
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
