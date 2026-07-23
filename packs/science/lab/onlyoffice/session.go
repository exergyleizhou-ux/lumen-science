package onlyoffice

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
)

// CallbackToken returns the optional shared secret for callback auth.
func CallbackToken() string {
	return strings.TrimSpace(os.Getenv("LUMEN_ONLYOFFICE_CALLBACK_TOKEN"))
}

// BuildCallbackURL builds the absolute callback URL including optional token.
// publicBase should be like "http://127.0.0.1:18992" or "https://demo.../lumen-lab"
// (no trailing slash). path is workspace-relative.
func BuildCallbackURL(publicBase, projectID, relPath string) (string, error) {
	base := strings.TrimRight(strings.TrimSpace(publicBase), "/")
	if base == "" {
		return "", fmt.Errorf("public base URL required")
	}
	pid := strings.TrimSpace(projectID)
	p := CallbackPath(relPath)
	if pid == "" || p == "" {
		return "", fmt.Errorf("project_id and path required")
	}
	u, err := url.Parse(base + "/api/lab/onlyoffice/callback")
	if err != nil {
		return "", err
	}
	q := u.Query()
	q.Set("project_id", pid)
	q.Set("path", p)
	if tok := CallbackToken(); tok != "" {
		q.Set("token", tok)
	}
	u.RawQuery = q.Encode()
	return u.String(), nil
}

// DocumentKey builds a stable OnlyOffice document key from project, path and
// file identity (size+mtime). Key changes after save so the editor reloads.
// OnlyOffice allows [0-9a-zA-Z._=-] up to 128 chars.
func DocumentKey(projectID, relPath string, size, mtimeUnix int64) string {
	raw := fmt.Sprintf("%s|%s|%d|%d", projectID, CallbackPath(relPath), size, mtimeUnix)
	sum := sha256.Sum256([]byte(raw))
	// 32 hex chars — well under 128 and charset-safe
	return hex.EncodeToString(sum[:16])
}

// DocumentKeyFromFile stats path under workspace abs root.
func DocumentKeyFromFile(projectID, relPath, absFile string) string {
	st, err := os.Stat(absFile)
	if err != nil {
		return DocumentKey(projectID, relPath, 0, 0)
	}
	return DocumentKey(projectID, relPath, st.Size(), st.ModTime().Unix())
}

// PublicBaseFromRequest derives the external Lab base URL from the request
// (supports reverse proxies via X-Forwarded-*).
func PublicBaseFromRequest(r *http.Request) string {
	if r == nil {
		return ""
	}
	proto := strings.TrimSpace(r.Header.Get("X-Forwarded-Proto"))
	if proto == "" {
		if r.TLS != nil {
			proto = "https"
		} else {
			proto = "http"
		}
	}
	host := strings.TrimSpace(r.Header.Get("X-Forwarded-Host"))
	if host == "" {
		host = strings.TrimSpace(r.Host)
	}
	if host == "" {
		return ""
	}
	// Optional path prefix (e.g. /lumen-lab behind Caddy)
	prefix := strings.TrimSpace(r.Header.Get("X-Forwarded-Prefix"))
	if prefix == "" {
		// Also honor a dedicated env for reverse-proxy mounts
		prefix = strings.TrimSpace(os.Getenv("LUMEN_PUBLIC_PATH_PREFIX"))
	}
	prefix = strings.TrimRight(prefix, "/")
	return strings.TrimRight(proto+"://"+host+prefix, "/")
}

// RewriteBaseHost rewrites the host of an absolute URL (e.g. to host.docker.internal).
func RewriteBaseHost(absURL, newHost string) string {
	u, err := url.Parse(absURL)
	if err != nil || newHost == "" {
		return absURL
	}
	// newHost may be "host.docker.internal:18992"
	u.Host = newHost
	return u.String()
}

// Session describes parameters the SPA needs to open the editor.
type Session struct {
	OK           bool   `json:"ok"`
	Mode         string `json:"mode"`
	DSURL        string `json:"ds_url,omitempty"`
	CallbackURL  string `json:"callback_url,omitempty"`
	DocumentKey  string `json:"document_key,omitempty"`
	DownloadPath string `json:"download_path,omitempty"` // relative API path with query
	FileType     string `json:"file_type,omitempty"`
	DocumentType string `json:"document_type,omitempty"`
	Title        string `json:"title,omitempty"`
	Path         string `json:"path,omitempty"`
	Error        string `json:"error,omitempty"`
	TokenRequired bool  `json:"token_required"`
}

// BuildSession constructs editor session metadata for a workspace file.
// absWorkspace is the project workspace absolute directory.
func BuildSession(publicBase, projectID, relPath, mode, absWorkspace string) Session {
	mode = strings.ToLower(strings.TrimSpace(mode))
	if mode != "edit" {
		mode = "view"
	}
	p := CallbackPath(relPath)
	if p == "" {
		return Session{OK: false, Error: "invalid path"}
	}
	ds := strings.TrimSpace(os.Getenv("LUMEN_ONLYOFFICE_URL"))
	if ds == "" {
		return Session{OK: false, Error: "LUMEN_ONLYOFFICE_URL not configured"}
	}
	ext := strings.TrimPrefix(filepath.Ext(p), ".")
	title := filepath.Base(p)
	abs := filepath.Join(absWorkspace, filepath.FromSlash(p))
	key := DocumentKeyFromFile(projectID, p, abs)

	s := Session{
		OK:            true,
		Mode:          mode,
		DSURL:         strings.TrimRight(ds, "/"),
		DocumentKey:   key,
		DownloadPath:  "/api/lab/files/download?project_id=" + url.QueryEscape(projectID) + "&path=" + url.QueryEscape(p),
		FileType:      FileTypeFromExt(ext),
		DocumentType:  DocTypeFromExt(ext),
		Title:         title,
		Path:          p,
		TokenRequired: CallbackToken() != "",
	}
	if mode == "edit" {
		cb, err := BuildCallbackURL(publicBase, projectID, p)
		if err != nil {
			return Session{OK: false, Error: err.Error()}
		}
		s.CallbackURL = cb
	}
	return s
}
