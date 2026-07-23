// Package onlyoffice handles the Document Server callback for edit persistence.
// When a user edits a document through OnlyOffice, the Document Server POSTs to
// the callback URL with the modified file's download location. This package
// fetches the file and writes it back into the project workspace.
package onlyoffice

import (
	"encoding/json"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"lumen/internal/science/lab/workspace"
)

// CallbackBody is the JSON payload OnlyOffice Document Server POSTs on save.
type CallbackBody struct {
	Status     int    `json:"status"`               // 1=editing, 2=ready, 6=force-save
	URL        string `json:"url,omitempty"`         // download URL of the modified file
	Key        string `json:"key,omitempty"`         // document identifier
	History    any    `json:"history,omitempty"`
	Actions    any    `json:"actions,omitempty"`
	Users      any    `json:"users,omitempty"`
	ChangesURL string `json:"changesurl,omitempty"`
}

// CallbackResponse is returned to the Document Server.
type CallbackResponse struct {
	Error int `json:"error"` // 0 = success
}

// HandleCallback processes a POST from OnlyOffice Document Server.
//
// Query params:
//   - project_id: project slug
//   - path: workspace-relative path to save to
//
// Security: LUMEN_ONLYOFFICE_CALLBACK_TOKEN enables shared-secret auth via
// query param ?token=...
func HandleCallback(w http.ResponseWriter, r *http.Request, g *workspace.Guard, relPath string) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Optional token check
	if token := os.Getenv("LUMEN_ONLYOFFICE_CALLBACK_TOKEN"); token != "" {
		if r.URL.Query().Get("token") != token {
			writeCB(w, 1) // 1 = invalid token (DS may retry)
			return
		}
	}

	if relPath == "" {
		writeCB(w, 1)
		return
	}

	// Read and parse callback body
	var cb CallbackBody
	if err := json.NewDecoder(io.LimitReader(r.Body, 64<<10)).Decode(&cb); err != nil {
		writeCB(w, 1)
		return
	}

	// Only persist on save-ready statuses
	if cb.Status != 2 && cb.Status != 6 {
		// status 1 (editing) or other — acknowledge without action
		writeCB(w, 0)
		return
	}

	if cb.URL == "" {
		writeCB(w, 1)
		return
	}

	// SSRF guard: only fetch URLs that look like the configured Document Server
	// (or common Docker Desktop host aliases).
	if err := ValidateDownloadURL(cb.URL); err != nil {
		writeCB(w, 1)
		return
	}

	// Download the modified file from the Document Server
	client := &http.Client{Timeout: 60 * time.Second}
	resp, err := client.Get(cb.URL)
	if err != nil {
		writeCB(w, 1)
		return
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		writeCB(w, 1)
		return
	}

	// Read up to 32 MB
	body, err := io.ReadAll(io.LimitReader(resp.Body, 32<<20))
	if err != nil {
		writeCB(w, 1)
		return
	}

	// Resolve workspace path (Guard prevents traversal)
	abs, err := g.Resolve(relPath)
	if err != nil {
		writeCB(w, 1)
		return
	}

	// Create parent directories if needed
	if err := os.MkdirAll(filepath.Dir(abs), 0o700); err != nil {
		writeCB(w, 1)
		return
	}

	if err := os.WriteFile(abs, body, 0o600); err != nil {
		writeCB(w, 1)
		return
	}

	writeCB(w, 0)
}

func writeCB(w http.ResponseWriter, code int) {
	// OnlyOffice expects HTTP 200 + {"error":N} even on logical failures.
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(CallbackResponse{Error: code})
}

// WriteCB is the exported form of writeCB for API handlers that need the same
// response shape without going through HandleCallback.
func WriteCB(w http.ResponseWriter, code int) {
	writeCB(w, code)
}

// CallbackPath helper to parse and validate the workspace path from the request.
// Returns the cleaned relative path, or empty string on failure.
func CallbackPath(raw string) string {
	p := filepath.ToSlash(filepath.Clean(strings.TrimSpace(raw)))
	if p == "" || p == "." || strings.HasPrefix(p, "..") || strings.HasPrefix(p, "/") {
		return ""
	}
	return p
}

// DocTypeFromExt maps a file extension to the OnlyOffice documentType.
func DocTypeFromExt(ext string) string {
	switch strings.ToLower(strings.TrimPrefix(ext, ".")) {
	case "xlsx":
		return "cell"
	case "pptx":
		return "slide"
	default:
		return "word"
	}
}

// FileTypeFromExt maps a file extension to the OnlyOffice fileType.
func FileTypeFromExt(ext string) string {
	e := strings.ToLower(strings.TrimPrefix(ext, "."))
	if e == "" {
		return "docx"
	}
	return e
}

// EditEnabled returns true when the Lab can support OnlyOffice editing
// (DS URL configured and callback token set if required).
func EditEnabled() bool {
	return strings.TrimSpace(os.Getenv("LUMEN_ONLYOFFICE_URL")) != ""
}

// EditHint returns a human-readable status for the edit capability.
func EditHint() string {
	if !EditEnabled() {
		return "设置 LUMEN_ONLYOFFICE_URL 以启用编辑"
	}
	return "编辑回写已启用（callback 就绪）"
}

// Health returns health information for the onlyoffice section.
func Health(dsURL string) map[string]any {
	hasURL := strings.TrimSpace(dsURL) != ""
	editOK := hasURL
	return map[string]any{
		"url":        dsURL,
		"configured": hasURL,
		"edit":       editOK,
		"editHint":   EditHint(),
	}
}

// FormatPathForEdit sanitizes a path for use as a callback path parameter.
func FormatPathForEdit(path string) string {
	p := filepath.ToSlash(filepath.Clean(strings.TrimSpace(path)))
	p = strings.TrimSuffix(p, "/")
	// Remove leading ./
	for strings.HasPrefix(p, "./") {
		p = p[2:]
	}
	return p
}
