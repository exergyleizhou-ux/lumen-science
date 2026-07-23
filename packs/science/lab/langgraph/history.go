package langgraph

import (
	"bufio"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// HistoryEntry is one sidecar run persisted per project.
type HistoryEntry struct {
	ID        string `json:"id"`
	TS        int64  `json:"ts"`
	ProjectID string `json:"project_id"`
	Prompt    string `json:"prompt"`
	OK        bool   `json:"ok"`
	Result    string `json:"result,omitempty"`
	Error     string `json:"error,omitempty"`
	Mode      string `json:"mode,omitempty"` // heuristic | llm
}

const (
	historyFileName = "langgraph-history.jsonl"
	historyMaxLines = 100
	historyMaxResult = 12000
)

// HistoryPath returns the jsonl path under a project directory.
func HistoryPath(projectDir string) string {
	return filepath.Join(projectDir, historyFileName)
}

// AppendHistory appends one entry and trims the file to historyMaxLines.
func AppendHistory(projectDir string, e HistoryEntry) error {
	if strings.TrimSpace(projectDir) == "" {
		return nil
	}
	if err := os.MkdirAll(projectDir, 0o700); err != nil {
		return err
	}
	if e.ID == "" {
		e.ID = "lg_" + time.Now().UTC().Format("20060102T150405") + "_" + randomSuffix()
	}
	if e.TS == 0 {
		e.TS = time.Now().UnixMilli()
	}
	if len(e.Result) > historyMaxResult {
		e.Result = e.Result[:historyMaxResult] + "\n…(截断)"
	}
	if len(e.Prompt) > 2000 {
		e.Prompt = e.Prompt[:2000]
	}
	path := HistoryPath(projectDir)
	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	enc := json.NewEncoder(f)
	if err := enc.Encode(e); err != nil {
		f.Close()
		return err
	}
	f.Close()
	return trimHistoryFile(path, historyMaxLines)
}

// ListHistory returns newest-first entries (up to limit).
func ListHistory(projectDir string, limit int) ([]HistoryEntry, error) {
	if limit <= 0 {
		limit = 40
	}
	path := HistoryPath(projectDir)
	f, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	defer f.Close()
	var all []HistoryEntry
	sc := bufio.NewScanner(f)
	// raise token size for long results
	buf := make([]byte, 0, 64*1024)
	sc.Buffer(buf, 2*1024*1024)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" {
			continue
		}
		var e HistoryEntry
		if err := json.Unmarshal([]byte(line), &e); err != nil {
			continue
		}
		all = append(all, e)
	}
	// newest last in file → reverse
	for i, j := 0, len(all)-1; i < j; i, j = i+1, j-1 {
		all[i], all[j] = all[j], all[i]
	}
	if len(all) > limit {
		all = all[:limit]
	}
	return all, sc.Err()
}

func trimHistoryFile(path string, max int) error {
	entries, err := ListHistory(filepath.Dir(path), max*2)
	if err != nil || len(entries) <= max {
		return err
	}
	// ListHistory returns newest first; keep first max, rewrite oldest→newest
	keep := entries[:max]
	for i, j := 0, len(keep)-1; i < j; i, j = i+1, j-1 {
		keep[i], keep[j] = keep[j], keep[i]
	}
	tmp := path + ".tmp"
	f, err := os.OpenFile(tmp, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o600)
	if err != nil {
		return err
	}
	enc := json.NewEncoder(f)
	for _, e := range keep {
		if err := enc.Encode(e); err != nil {
			f.Close()
			return err
		}
	}
	f.Close()
	return os.Rename(tmp, path)
}

func randomSuffix() string {
	// short non-crypto suffix
	return time.Now().Format("150405.000")
}
